//! The cash flow (phase 25 PR B, sheafly-pro#5): money in from the payments, money out from
//! the costs and the balance by period, computed by the closed-source Pro crate
//! (`sheafly_pro::cash_flow`). A read, never logged. The payment data itself is Free and goes
//! through `apply_command` like any edit; only this read is gated (`Feature::Cashflow`).
//! Builds from the public source say they have no Pro modules before asking for a license.
//! `payment_dates` is Free (D7): the payment dialog shows when each payment is due.
//! `export_cash_flow` (PR D) saves the same report as an Excel workbook, behind the same gate.

use std::fs;

use serde::Deserialize;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use super::{await_save, begin_dialog, ensure_suffix, i18n, lang, lock, path_from_dialog, write_failed, AppState};
use sheafly_engine::cash_flow::{CashFlowOptions, CashFlowReport};
use sheafly_engine::payments::{self, DateBasis, PaymentAnchor, PaymentDates};
use sheafly_engine::{Lang, Project};
use sheafly_license::Feature;

/// An anchor and its lag, as the payment dialog asks for its dates.
#[derive(Deserialize)]
pub(crate) struct DatesQuery {
    anchor: PaymentAnchor,
    #[serde(default)]
    lag_days: u16,
}

/// When each anchor is invoiced and due as scheduled now, in the order asked; `None` for a
/// task that is gone. A read, never logged, and not gated: the data it reads is Free.
#[tauri::command(async)]
pub(crate) fn payment_dates(state: State<'_, AppState>, items: Vec<DatesQuery>) -> Vec<Option<PaymentDates>> {
    dates_for(&lock(&state).project, &items)
}

fn dates_for(project: &Project, items: &[DatesQuery]) -> Vec<Option<PaymentDates>> {
    items
        .iter()
        .map(|item| payments::payment_dates(project, item.anchor, item.lag_days, DateBasis::Scheduled))
        .collect()
}

#[tauri::command]
pub(crate) async fn cash_flow(
    state: State<'_, AppState>,
    options: CashFlowOptions,
) -> Result<CashFlowReport, String> {
    let l = lang(&state);
    gate(sheafly_pro::available(), state.license.grants(Feature::Cashflow), l)?;
    // Three cost series over every leaf: off the main thread.
    let project = lock(&state).project.clone();
    tauri::async_runtime::spawn_blocking(move || sheafly_pro::cash_flow(&project, &options, l))
        .await
        .map_err(|error| error.to_string())?
}

/// Save the cash flow for `options` (the Dòng tiền view's period and baseline) as an
/// `.xlsx` workbook, like `export_excel`.
#[tauri::command]
pub(crate) async fn export_cash_flow(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    options: CashFlowOptions,
) -> Result<(), String> {
    let l = lang(&state);
    gate(sheafly_pro::available(), state.license.grants(Feature::Cashflow), l)?;
    let Some(_permit) = begin_dialog(&state) else {
        return Ok(());
    };
    let text = i18n::text(l);
    let picked = await_save(
        app.dialog()
            .file()
            .add_filter("Excel", &["xlsx"])
            .set_title(text.export_cash_flow_title)
            .set_file_name(text.export_cash_flow_file),
    )
    .await;
    let Some(path) = picked.and_then(path_from_dialog) else {
        return Ok(());
    };
    let path = ensure_suffix(path, ".xlsx");
    let project = lock(&state).project.clone();
    let bytes = tauri::async_runtime::spawn_blocking(move || sheafly_pro::cash_flow_workbook(&project, &options, l))
        .await
        .map_err(|error| error.to_string())??;
    fs::write(path, bytes).map_err(write_failed(&state))
}

/// Whether the read may run: the Pro modules first, then the license.
fn gate(pro: bool, granted: bool, l: Lang) -> Result<(), String> {
    if !pro {
        return Err(sheafly_pro::unavailable(l));
    }
    if !granted {
        return Err(i18n::text(l).needs_pro_cashflow.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_modules_come_before_the_license_in_the_language_asked_for() {
        assert_eq!(gate(false, true, Lang::Vi), Err(sheafly_pro::unavailable(Lang::Vi)));
        assert_eq!(gate(false, false, Lang::En), Err(sheafly_pro::unavailable(Lang::En)));
        assert_eq!(gate(true, false, Lang::En), Err(i18n::text(Lang::En).needs_pro_cashflow.into()));
        assert_eq!(gate(true, false, Lang::Vi), Err(i18n::text(Lang::Vi).needs_pro_cashflow.into()));
        assert_eq!(gate(true, true, Lang::Vi), Ok(()));
    }

    #[test]
    fn dates_come_in_the_order_asked_and_none_for_a_task_that_is_gone() {
        let start = sheafly_engine::timeutil::timestamp(2026, 1, 5, 8, 0);
        let project = sheafly_engine::Document::new_in(Lang::Vi).project;
        let uid = project.tasks[0].uid;
        let finish = project.tasks[0].finish;
        let items = [
            DatesQuery { anchor: PaymentAnchor::Finish { task: uid }, lag_days: 30 },
            DatesQuery { anchor: PaymentAnchor::Finish { task: uid + 99 }, lag_days: 0 },
            DatesQuery { anchor: PaymentAnchor::Date { at: start }, lag_days: 1 },
        ];
        let dates = dates_for(&project, &items);
        assert_eq!(dates.len(), 3);
        let first = dates[0].expect("the task is there");
        assert_eq!(first.invoice, finish);
        assert_eq!(first.due, payments::due_date(finish, 30));
        assert!(!first.actual);
        assert_eq!(dates[1], None);
        assert_eq!(dates[2].map(|d| d.due), Some(payments::due_date(start, 1)));
    }
}
