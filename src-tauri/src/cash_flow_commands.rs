//! The cash flow (phase 25 PR B, sheafly-pro#5): money in from the payments, money out from
//! the costs and the balance by period, computed by the closed-source Pro crate
//! (`sheafly_pro::cash_flow`). A read, never logged. The payment data itself is Free and goes
//! through `apply_command` like any edit; only this read is gated (`Feature::Cashflow`).
//! Builds from the public source say they have no Pro modules before asking for a license.

use tauri::State;

use super::{i18n, lang, lock, AppState};
use sheafly_engine::cash_flow::{CashFlowOptions, CashFlowReport};
use sheafly_engine::Lang;
use sheafly_license::Feature;

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
}
