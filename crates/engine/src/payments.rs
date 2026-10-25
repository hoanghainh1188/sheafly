//! Contract value and payments by milestone (phase 25, sheafly-pro#5): the data of the cash
//! flow. Free, like every project field: the cash flow view that reads it is Pro.
//!
//! A payment belongs to the project, not to a task (D2): it is anchored to a task's finish
//! (a milestone or a group, by default), to a task's start (an advance), or to a fixed date.
//! It is invoiced on that date and due `lag_days` calendar days later (Net 30, D3), at the
//! start of that day. Its amount is fixed or a percent of the contract value. A retention
//! keeps back a percent of each `retained` payment and releases it once, at its own anchor.
//!
//! `SetPayments` replaces the list like `SetAssignmentRules` (an entry with an id keeps
//! that payment and its receipts, one without is new; ids are never reused); receipts are
//! recorded apart (`SetPaymentReceipts`). When a command removes an anchored task (deleting
//! it, its group or its series, or regenerating a series without it), the payment becomes a
//! fixed date on the day it would have been invoiced just before, marked `detached` (D5), and
//! a retention released at it does the same; undo puts it back, since undo is a replay.
//!
//! Scheduling never reads any of this, and MSPDI has no element for it: it lives in
//! `.sheafly` files only, and copying tasks does not carry it.

use std::collections::HashSet;

use chrono::{Datelike, Days};
use serde::{Deserialize, Serialize};

use crate::error::{EngineError, Invalid};
use crate::model::Project;
use crate::timeutil::{civil, compose};

/// At most this many payments in a project.
pub const MAX_PAYMENTS: usize = 500;
/// At most this many receipts for one payment.
pub const MAX_RECEIPTS: usize = 500;
/// The longest payment name, in characters.
pub const MAX_PAYMENT_NAME: usize = 200;
/// The longest time from invoice to due date, in calendar days.
pub const MAX_LAG_DAYS: u16 = 365;
/// The largest retention, in percent.
pub const MAX_RETENTION_PERCENT: f64 = 50.0;
/// The largest amount of money, so sums stay exact enough.
pub const MAX_AMOUNT: f64 = 1e15;

/// The project's contract and opening balance. Empty by default.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CashFlowSettings {
    /// What the contract is worth; percent payments are shares of it.
    #[serde(default)]
    pub contract_value: Option<f64>,
    /// The balance before the project's first cash flow; may be negative.
    #[serde(default)]
    pub opening_balance: f64,
    #[serde(default)]
    pub retention: Option<Retention>,
}

/// A share kept back from each `retained` payment and released once.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Retention {
    /// 0 to 50.
    pub percent: f64,
    /// When the kept money is invoiced.
    pub release: PaymentAnchor,
    #[serde(default)]
    pub lag_days: u16,
}

/// How much a payment is.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentAmount {
    /// An amount of money.
    Fixed(f64),
    /// A percent (0 to 100) of the contract value; nothing without one.
    Percent(f64),
}

/// The date a payment is invoiced on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaymentAnchor {
    /// When the task finishes (a milestone or a group, typically).
    Finish { task: u32 },
    /// When the task starts (an advance).
    Start { task: u32 },
    /// A fixed date (signing the contract).
    Date { at: i64 },
}

impl PaymentAnchor {
    /// The task the anchor follows, if any.
    pub fn task(self) -> Option<u32> {
        match self {
            Self::Finish { task } | Self::Start { task } => Some(task),
            Self::Date { .. } => None,
        }
    }
}

/// Money received for a payment.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    pub date: i64,
    pub amount: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Payment {
    pub id: u32,
    pub name: String,
    pub amount: PaymentAmount,
    pub anchor: PaymentAnchor,
    /// Calendar days from the invoice to the due date.
    #[serde(default)]
    pub lag_days: u16,
    /// The retention keeps back its share of this payment.
    #[serde(default)]
    pub retained: bool,
    #[serde(default)]
    pub received: Vec<Receipt>,
    /// Was anchored to a task that is gone; now a fixed date.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub detached: bool,
}

impl Payment {
    /// The money the payment is worth before any retention.
    pub fn value(&self, settings: &CashFlowSettings) -> f64 {
        match self.amount {
            PaymentAmount::Fixed(amount) => amount,
            PaymentAmount::Percent(percent) => {
                settings.contract_value.unwrap_or(0.0) * percent / 100.0
            }
        }
    }

    /// The money received so far.
    pub fn received_total(&self) -> f64 {
        self.received.iter().map(|r| r.amount).sum()
    }
}

/// A payment as `SetPayments` takes it: `id` keeps an existing payment (and its receipts),
/// none makes a new one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaymentInput {
    #[serde(default)]
    pub id: Option<u32>,
    pub name: String,
    pub amount: PaymentAmount,
    pub anchor: PaymentAnchor,
    #[serde(default)]
    pub lag_days: u16,
    #[serde(default)]
    pub retained: bool,
}

/// Which dates of an anchor to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DateBasis {
    /// As scheduled now; a task's actual start or finish once it has one.
    Scheduled,
    /// Baseline n of the anchored task.
    Baseline { number: u8 },
}

/// When a payment is invoiced and due.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentDates {
    pub invoice: i64,
    /// The start of the day `lag_days` calendar days after the invoice's.
    pub due: i64,
    /// The invoice date is the task's actual start or finish.
    pub actual: bool,
}

/// The date `anchor` invoices on, by `basis`: `None` for a task that is gone, or that has
/// no such baseline. A fixed date is the same for every basis.
pub fn anchor_date(
    project: &Project,
    anchor: PaymentAnchor,
    basis: DateBasis,
) -> Option<(i64, bool)> {
    let (uid, finish) = match anchor {
        PaymentAnchor::Date { at } => return Some((at, false)),
        PaymentAnchor::Finish { task } => (task, true),
        PaymentAnchor::Start { task } => (task, false),
    };
    let task = project.task(uid)?;
    match basis {
        DateBasis::Scheduled => {
            let actual = if finish {
                task.actual_finish
            } else {
                task.actual_start
            };
            Some(match actual {
                Some(at) => (at, true),
                None => (if finish { task.finish } else { task.start }, false),
            })
        }
        DateBasis::Baseline { number } => {
            let b = task.baselines.iter().find(|b| b.number == number)?;
            Some((if finish { b.finish } else { b.start }, false))
        }
    }
}

/// When a payment anchored at `anchor` with `lag_days` is invoiced and due, by `basis`.
pub fn payment_dates(
    project: &Project,
    anchor: PaymentAnchor,
    lag_days: u16,
    basis: DateBasis,
) -> Option<PaymentDates> {
    let (invoice, actual) = anchor_date(project, anchor, basis)?;
    Some(PaymentDates {
        invoice,
        due: due_date(invoice, lag_days),
        actual,
    })
}

/// The start of the day `lag_days` calendar days after `invoice`'s.
pub fn due_date(invoice: i64, lag_days: u16) -> i64 {
    let day = civil(invoice).0;
    let due = day
        .checked_add_days(Days::new(u64::from(lag_days)))
        .unwrap_or(day);
    compose(due, 0)
}

fn check_amount(amount: f64) -> Result<(), EngineError> {
    if amount.is_finite() && (0.0..=MAX_AMOUNT).contains(&amount) {
        Ok(())
    } else {
        Err(Invalid::PaymentAmount.into())
    }
}

/// A date between the years 1900 and 2200, so due dates and periods stay in range.
fn check_date(at: i64) -> Result<(), EngineError> {
    let year = civil(at).0.year();
    let back = compose(civil(at).0, civil(at).1);
    if (1900..=2200).contains(&year) && back / 60_000 == at / 60_000 {
        Ok(())
    } else {
        Err(Invalid::BadDate.into())
    }
}

fn check_anchor(project: &Project, anchor: PaymentAnchor) -> Result<(), EngineError> {
    match anchor {
        PaymentAnchor::Date { at } => check_date(at),
        PaymentAnchor::Finish { task } | PaymentAnchor::Start { task } => project
            .task(task)
            .map(|_| ())
            .ok_or(EngineError::TaskNotFound),
    }
}

fn check_lag(lag_days: u16) -> Result<(), EngineError> {
    if lag_days <= MAX_LAG_DAYS {
        Ok(())
    } else {
        Err(Invalid::PaymentLag.into())
    }
}

/// `SetCashFlowSettings`: replace the contract value, opening balance and retention.
pub fn set_settings(project: &mut Project, settings: &CashFlowSettings) -> Result<(), EngineError> {
    if let Some(value) = settings.contract_value {
        check_amount(value)?;
    }
    let balance = settings.opening_balance;
    if !balance.is_finite() || balance.abs() > MAX_AMOUNT {
        return Err(Invalid::PaymentAmount.into());
    }
    if let Some(retention) = &settings.retention {
        let percent = retention.percent;
        if !percent.is_finite() || !(0.0..=MAX_RETENTION_PERCENT).contains(&percent) {
            return Err(Invalid::RetentionPercent.into());
        }
        check_anchor(project, retention.release)?;
        check_lag(retention.lag_days)?;
    }
    project.cash_flow = settings.clone();
    Ok(())
}

/// `SetPayments`: replace the list (see the module header).
pub fn set_payments(project: &mut Project, inputs: &[PaymentInput]) -> Result<(), EngineError> {
    if inputs.len() > MAX_PAYMENTS {
        return Err(Invalid::TooManyPayments.into());
    }
    let mut next_id = next_id(project);
    let mut kept_ids = Vec::new();
    let mut payments = Vec::with_capacity(inputs.len());
    for input in inputs {
        let name = input.name.trim();
        if name.is_empty() || name.chars().count() > MAX_PAYMENT_NAME {
            return Err(Invalid::PaymentName.into());
        }
        match input.amount {
            PaymentAmount::Fixed(amount) => check_amount(amount)?,
            PaymentAmount::Percent(percent) => {
                if !percent.is_finite() || !(0.0..=100.0).contains(&percent) {
                    return Err(Invalid::PaymentAmount.into());
                }
            }
        }
        check_anchor(project, input.anchor)?;
        check_lag(input.lag_days)?;
        let (id, received, detached) = match input.id {
            Some(id) => {
                if kept_ids.contains(&id) {
                    return Err(Invalid::PaymentRepeated.into());
                }
                kept_ids.push(id);
                let old = project
                    .payments
                    .iter()
                    .find(|p| p.id == id)
                    .ok_or(Invalid::NoSuchPayment)?;
                (
                    id,
                    old.received.clone(),
                    old.detached && old.anchor == input.anchor,
                )
            }
            None => {
                next_id += 1;
                (next_id - 1, Vec::new(), false)
            }
        };
        payments.push(Payment {
            id,
            name: name.to_string(),
            amount: input.amount,
            anchor: input.anchor,
            lag_days: input.lag_days,
            retained: input.retained,
            received,
            detached,
        });
    }
    project.payments = payments;
    project.next_payment_id = next_id;
    Ok(())
}

/// The id the next payment gets: never one a payment has had (0 in an older project means
/// one past the largest).
fn next_id(project: &Project) -> u32 {
    let past_largest = project.payments.iter().map(|p| p.id + 1).max().unwrap_or(1);
    project.next_payment_id.max(past_largest)
}

/// `SetPaymentReceipts`: replace the money received for payment `id`. Receiving more than
/// the payment is allowed; the cash flow notes it.
pub fn set_receipts(
    project: &mut Project,
    id: u32,
    receipts: &[Receipt],
) -> Result<(), EngineError> {
    if receipts.len() > MAX_RECEIPTS {
        return Err(Invalid::TooManyReceipts.into());
    }
    for receipt in receipts {
        check_date(receipt.date)?;
        check_amount(receipt.amount)?;
    }
    let payment = project
        .payments
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or(Invalid::NoSuchPayment)?;
    let mut sorted = receipts.to_vec();
    sorted.sort_by_key(|r| r.date);
    payment.received = sorted;
    Ok(())
}

/// The invoice dates of the anchors that follow a task, as scheduled before a command, for
/// `detach_gone` after it. Empty (and free) when nothing is anchored to a task; otherwise
/// one pass over the tasks with a set of at most 501 uids.
pub(crate) fn anchored_dates(project: &Project) -> Vec<(PaymentAnchor, i64)> {
    let anchors = task_anchors(project);
    if anchors.is_empty() {
        return Vec::new();
    }
    let uids: HashSet<u32> = anchors.iter().filter_map(|a| a.task()).collect();
    let mut dates = Vec::with_capacity(anchors.len());
    for task in project.tasks.iter().filter(|t| uids.contains(&t.uid)) {
        let start = task.actual_start.unwrap_or(task.start);
        let finish = task.actual_finish.unwrap_or(task.finish);
        dates.push((PaymentAnchor::Start { task: task.uid }, start));
        dates.push((PaymentAnchor::Finish { task: task.uid }, finish));
    }
    dates
}

/// The anchors of the payments and the retention that follow a task.
fn task_anchors(project: &Project) -> Vec<PaymentAnchor> {
    let retention = project.cash_flow.retention.as_ref().map(|r| r.release);
    project
        .payments
        .iter()
        .map(|p| p.anchor)
        .chain(retention)
        .filter(|anchor| anchor.task().is_some())
        .collect()
}

/// Turns anchors whose task a command removed into fixed dates on the day they were
/// invoiced before it (`before`, from `anchored_dates`), marking payments `detached`.
pub(crate) fn detach_gone(project: &mut Project, before: &[(PaymentAnchor, i64)]) {
    if before.is_empty() {
        return;
    }
    let anchored: HashSet<u32> = before.iter().filter_map(|(a, _)| a.task()).collect();
    let present: HashSet<u32> = project
        .tasks
        .iter()
        .map(|t| t.uid)
        .filter(|uid| anchored.contains(uid))
        .collect();
    let gone = |anchor: PaymentAnchor| {
        anchor.task().filter(|uid| !present.contains(uid))?;
        before.iter().find(|(a, _)| *a == anchor).map(|(_, at)| *at)
    };
    for payment in &mut project.payments {
        if let Some(at) = gone(payment.anchor) {
            payment.anchor = PaymentAnchor::Date { at };
            payment.detached = true;
        }
    }
    if let Some(retention) = project.cash_flow.retention.as_mut() {
        if let Some(at) = gone(retention.release) {
            retention.release = PaymentAnchor::Date { at };
        }
    }
}
