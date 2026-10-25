//! The types of the cash flow report (phase 25, sheafly-pro#5). The report itself, periods,
//! retention, overdue payments, balance and the comparison with a baseline, is computed by
//! the private Pro crate (`sheafly_pro::cash_flow`); these types are public so the stub, the
//! private crate and the shell share one definition (like `health.rs`). The data it reads is
//! `payments.rs`; money out is `cost_timephased.rs`.

use serde::{Deserialize, Serialize};

use crate::load_map::Bucket;
use crate::payments::PaymentDates;

/// What to compute: the period, the baseline to compare with, and the span (by default
/// from the project start, or the earliest money in or out, to the latest due date).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashFlowOptions {
    pub bucket: Bucket,
    #[serde(default)]
    pub baseline: Option<u8>,
    #[serde(default)]
    pub from: Option<i64>,
    #[serde(default)]
    pub to: Option<i64>,
}

/// Money in one period, by series. `actual` exists up to the status date, `baseline` when a
/// baseline is chosen.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Flows {
    pub forecast: f64,
    #[serde(default)]
    pub actual: Option<f64>,
    #[serde(default)]
    pub baseline: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CashFlowPeriod {
    /// The start of the period's first day.
    pub date: i64,
    /// The first day, `YYYY-MM-DD`.
    pub label: String,
    pub inflow: Flows,
    pub outflow: Flows,
    /// Inflow minus outflow, by series.
    pub net: Flows,
    /// The balance at the end of the period, from the opening balance, by series.
    pub balance: Flows,
    /// The forecast balance is below zero.
    pub deficit: bool,
}

/// One payment (or the release of the retention, with no `id`) in the forecast.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaymentForecast {
    pub id: Option<u32>,
    pub name: String,
    /// What is due, after the retention.
    pub amount: f64,
    /// What the retention keeps back.
    pub retained: f64,
    pub received: f64,
    /// As scheduled now; none when the anchored task is gone.
    pub scheduled: Option<PaymentDates>,
    /// By the chosen baseline.
    pub baseline: Option<PaymentDates>,
    /// When the forecast expects what is still unpaid: the due date, or the status date
    /// once that is past (`overdue`).
    pub expected: Option<i64>,
    pub overdue: bool,
    /// Calendar days the scheduled due date is after the baseline's.
    pub late_days: Option<i64>,
    /// The tasks driving the anchor's date (`summary_tools::critical_chain`), as UIDs.
    #[serde(default)]
    pub driving: Vec<u32>,
}

/// Something worth telling about the payments.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CashFlowNote {
    /// The percent payments do not add up to 100.
    PercentTotal { total: f64 },
    /// A percent payment with no contract value counts as nothing.
    PercentWithoutContract { id: u32 },
    /// More was received than the payment is.
    OverReceived { id: u32 },
    /// The payment's task was deleted; it is now a fixed date.
    Detached { id: u32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CashFlowReport {
    pub bucket: Bucket,
    pub baseline: Option<u8>,
    pub status_date: Option<i64>,
    pub opening_balance: f64,
    pub periods: Vec<CashFlowPeriod>,
    pub payments: Vec<PaymentForecast>,
    /// The lowest forecast balance, and the first period it goes below zero.
    pub lowest_balance: Option<f64>,
    pub first_deficit: Option<i64>,
    #[serde(default)]
    pub notes: Vec<CashFlowNote>,
}
