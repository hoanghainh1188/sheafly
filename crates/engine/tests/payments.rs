//! Cash flow data (phase 25 PR A, sheafly-pro#5): the contract, payments by milestone or
//! date and the money received. Free data in `.sheafly` files only; nothing schedules
//! from it.

use chrono::Datelike;

use sheafly_engine::clipboard::copy_tasks;
use sheafly_engine::command::Command;
use sheafly_engine::document::replay;
use sheafly_engine::model::Task;
use sheafly_engine::mspdi::{read_mspdi, write_mspdi};
use sheafly_engine::payments::{
    due_date, payment_dates, CashFlowSettings, DateBasis, Payment, PaymentAmount, PaymentAnchor,
    PaymentDates, PaymentInput, Receipt, Retention, MAX_PAYMENTS,
};
use sheafly_engine::recurrence::{RecurrenceEnd, RecurrencePattern, RecurrenceSpec};
use sheafly_engine::store::{load, save};
use sheafly_engine::timeutil::timestamp;
use sheafly_engine::{Document, EngineError, Invalid, Project};

const DAY: i64 = 480;

/// Day `day` after Monday 2026-01-05, at `hour`.
fn at(day: u32, hour: u32) -> i64 {
    let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 5)
        .unwrap()
        .checked_add_signed(chrono::Duration::days(i64::from(day)))
        .unwrap();
    timestamp(date.year(), date.month(), date.day(), hour, 0)
}

/// Three one-day tasks, each after the one before: day 0, day 1, day 2.
fn doc() -> Document {
    let mut project = Project::new("Hợp đồng", at(0, 8));
    for uid in 1..=3 {
        let mut task = Task::new(uid, format!("Việc {uid}"));
        task.duration = DAY;
        task.remaining_duration = DAY;
        project.tasks.push(task);
    }
    project.next_task_uid = 4;
    let mut doc = Document::from_project(project);
    doc.apply(Command::SetPredecessors {
        uid: 2,
        text: "1".into(),
    })
    .unwrap();
    doc.apply(Command::SetPredecessors {
        uid: 3,
        text: "2".into(),
    })
    .unwrap();
    doc
}

fn input(
    id: Option<u32>,
    name: &str,
    amount: PaymentAmount,
    anchor: PaymentAnchor,
) -> PaymentInput {
    PaymentInput {
        id,
        name: name.into(),
        amount,
        anchor,
        lag_days: 0,
        retained: false,
    }
}

fn finish(task: u32) -> PaymentAnchor {
    PaymentAnchor::Finish { task }
}

fn set(doc: &mut Document, payments: Vec<PaymentInput>) -> Result<(), EngineError> {
    doc.apply(Command::SetPayments { payments }).map(|_| ())
}

fn invalid(result: Result<(), EngineError>) -> Invalid {
    match result {
        Err(EngineError::Invalid(reason)) => reason,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn ids(doc: &Document) -> Vec<u32> {
    doc.project.payments.iter().map(|p| p.id).collect()
}

#[test]
fn settings_are_checked_and_one_undo_step() {
    let mut doc = doc();
    let good = CashFlowSettings {
        contract_value: Some(1_000_000.0),
        opening_balance: -50_000.0,
        retention: Some(Retention {
            percent: 5.0,
            release: finish(3),
            lag_days: 30,
        }),
    };
    doc.apply(Command::SetCashFlowSettings {
        settings: good.clone(),
    })
    .unwrap();
    assert_eq!(doc.project.cash_flow, good);
    doc.undo().unwrap();
    assert_eq!(doc.project.cash_flow, CashFlowSettings::default());
    doc.redo().unwrap();
    assert_eq!(doc.project.cash_flow, good);

    let refused = |doc: &mut Document, settings: CashFlowSettings| {
        doc.apply(Command::SetCashFlowSettings { settings })
            .map(|_| ())
    };
    let bad_value = CashFlowSettings {
        contract_value: Some(-1.0),
        ..good.clone()
    };
    assert_eq!(
        invalid(refused(&mut doc, bad_value)),
        Invalid::PaymentAmount
    );
    let nan_balance = CashFlowSettings {
        opening_balance: f64::NAN,
        ..good.clone()
    };
    assert_eq!(
        invalid(refused(&mut doc, nan_balance)),
        Invalid::PaymentAmount
    );
    let mut big_retention = good.clone();
    big_retention.retention.as_mut().unwrap().percent = 50.5;
    assert_eq!(
        invalid(refused(&mut doc, big_retention)),
        Invalid::RetentionPercent
    );
    let mut long_lag = good.clone();
    long_lag.retention.as_mut().unwrap().lag_days = 366;
    assert_eq!(invalid(refused(&mut doc, long_lag)), Invalid::PaymentLag);
    let mut no_task = good.clone();
    no_task.retention.as_mut().unwrap().release = finish(99);
    assert!(matches!(
        refused(&mut doc, no_task),
        Err(EngineError::TaskNotFound)
    ));
    assert_eq!(doc.project.cash_flow, good);
}

#[test]
fn the_list_keeps_ids_and_receipts_and_never_reuses_an_id() {
    let mut doc = doc();
    set(
        &mut doc,
        vec![
            input(
                None,
                "Tạm ứng",
                PaymentAmount::Fixed(100.0),
                PaymentAnchor::Date { at: at(0, 8) },
            ),
            input(None, "Thiết kế", PaymentAmount::Percent(30.0), finish(2)),
        ],
    )
    .unwrap();
    assert_eq!(ids(&doc), [1, 2]);
    assert_eq!(doc.project.next_payment_id, 3);
    let receipts = vec![
        Receipt {
            date: at(3, 9),
            amount: 40.0,
        },
        Receipt {
            date: at(1, 9),
            amount: 60.0,
        },
    ];
    doc.apply(Command::SetPaymentReceipts { id: 1, receipts })
        .unwrap();
    // Receipts are kept in date order.
    assert_eq!(doc.project.payments[0].received[0].date, at(1, 9));
    assert_eq!(doc.project.payments[0].received_total(), 100.0);

    // Reorder, rename, drop payment 2, add one: the new one does not get id 2 back.
    set(
        &mut doc,
        vec![
            input(None, "Nghiệm thu", PaymentAmount::Percent(70.0), finish(3)),
            input(
                Some(1),
                "  Tạm ứng 10 %  ",
                PaymentAmount::Fixed(100.0),
                PaymentAnchor::Date { at: at(0, 8) },
            ),
        ],
    )
    .unwrap();
    assert_eq!(ids(&doc), [3, 1]);
    assert_eq!(doc.project.payments[1].name, "Tạm ứng 10 %");
    assert_eq!(doc.project.payments[1].received.len(), 2);
    set(
        &mut doc,
        vec![input(None, "Mới", PaymentAmount::Fixed(1.0), finish(1))],
    )
    .unwrap();
    assert_eq!(ids(&doc), [4]);

    // One undo step each, and the ids come back with them.
    doc.undo().unwrap();
    assert_eq!(ids(&doc), [3, 1]);
}

#[test]
fn payments_and_receipts_are_checked() {
    let mut doc = doc();
    let fixed = |amount| input(None, "Khoản", PaymentAmount::Fixed(amount), finish(1));
    assert_eq!(
        invalid(set(&mut doc, vec![fixed(-1.0)])),
        Invalid::PaymentAmount
    );
    assert_eq!(
        invalid(set(&mut doc, vec![fixed(f64::INFINITY)])),
        Invalid::PaymentAmount
    );
    let percent = input(None, "Khoản", PaymentAmount::Percent(100.5), finish(1));
    assert_eq!(
        invalid(set(&mut doc, vec![percent])),
        Invalid::PaymentAmount
    );
    let blank = input(None, "  ", PaymentAmount::Fixed(1.0), finish(1));
    assert_eq!(invalid(set(&mut doc, vec![blank])), Invalid::PaymentName);
    let long = input(None, &"đ".repeat(201), PaymentAmount::Fixed(1.0), finish(1));
    assert_eq!(invalid(set(&mut doc, vec![long])), Invalid::PaymentName);
    let ok_long = input(None, &"đ".repeat(200), PaymentAmount::Fixed(1.0), finish(1));
    set(&mut doc, vec![ok_long]).unwrap();
    let mut lag = fixed(1.0);
    lag.lag_days = 366;
    assert_eq!(invalid(set(&mut doc, vec![lag])), Invalid::PaymentLag);
    let gone = input(
        None,
        "Khoản",
        PaymentAmount::Fixed(1.0),
        PaymentAnchor::Start { task: 42 },
    );
    assert!(matches!(
        set(&mut doc, vec![gone]),
        Err(EngineError::TaskNotFound)
    ));
    let far = input(
        None,
        "Khoản",
        PaymentAmount::Fixed(1.0),
        PaymentAnchor::Date { at: i64::MAX },
    );
    assert_eq!(invalid(set(&mut doc, vec![far])), Invalid::BadDate);
    let unknown = input(Some(9), "Khoản", PaymentAmount::Fixed(1.0), finish(1));
    assert_eq!(
        invalid(set(&mut doc, vec![unknown])),
        Invalid::NoSuchPayment
    );
    let twice = input(Some(1), "Khoản", PaymentAmount::Fixed(1.0), finish(1));
    assert_eq!(
        invalid(set(&mut doc, vec![twice.clone(), twice])),
        Invalid::PaymentRepeated
    );
    let many = vec![fixed(1.0); MAX_PAYMENTS + 1];
    assert_eq!(invalid(set(&mut doc, many)), Invalid::TooManyPayments);
    set(&mut doc, vec![fixed(1.0); MAX_PAYMENTS]).unwrap();
    assert_eq!(doc.project.payments.len(), MAX_PAYMENTS);

    let receipts = |doc: &mut Document, id, receipts| {
        doc.apply(Command::SetPaymentReceipts { id, receipts })
            .map(|_| ())
    };
    let bad = vec![Receipt {
        date: at(0, 8),
        amount: -5.0,
    }];
    assert_eq!(invalid(receipts(&mut doc, 2, bad)), Invalid::PaymentAmount);
    let good = vec![Receipt {
        date: at(0, 8),
        amount: 5.0,
    }];
    assert_eq!(
        invalid(receipts(&mut doc, 9_999, good.clone())),
        Invalid::NoSuchPayment
    );
    // Receiving more than the payment is allowed (the cash flow notes it).
    let more = vec![Receipt {
        date: at(0, 8),
        amount: 50.0,
    }];
    receipts(&mut doc, 2, more).unwrap();
    let too_many = vec![good[0]; 501];
    assert_eq!(
        invalid(receipts(&mut doc, 2, too_many)),
        Invalid::TooManyReceipts
    );
}

#[test]
fn a_percent_is_a_share_of_the_contract_and_nothing_without_one() {
    let payment = |amount| Payment {
        id: 1,
        name: "Khoản".into(),
        amount,
        anchor: finish(1),
        lag_days: 0,
        retained: false,
        received: vec![],
        detached: false,
    };
    let none = CashFlowSettings::default();
    let contract = CashFlowSettings {
        contract_value: Some(2_000.0),
        ..none.clone()
    };
    assert_eq!(payment(PaymentAmount::Percent(30.0)).value(&none), 0.0);
    assert_eq!(
        payment(PaymentAmount::Percent(30.0)).value(&contract),
        600.0
    );
    assert_eq!(payment(PaymentAmount::Fixed(150.0)).value(&none), 150.0);
}

#[test]
fn dates_follow_the_schedule_then_the_actual_dates_and_baselines_stay() {
    let mut doc = doc();
    let on_finish = finish(2);
    let on_start = PaymentAnchor::Start { task: 2 };
    let scheduled = |doc: &Document, anchor, lag| {
        payment_dates(&doc.project, anchor, lag, DateBasis::Scheduled)
    };
    let base = |doc: &Document, anchor, number| {
        payment_dates(&doc.project, anchor, 0, DateBasis::Baseline { number })
    };
    assert_eq!(
        scheduled(&doc, on_finish, 30),
        Some(PaymentDates {
            invoice: at(1, 17),
            due: at(31, 0),
            actual: false
        })
    );
    assert_eq!(scheduled(&doc, on_start, 0).unwrap().invoice, at(1, 8));
    assert_eq!(base(&doc, on_finish, 0), None);

    doc.apply(Command::SetBaseline { number: 0 }).unwrap();
    doc.apply(Command::SetDuration {
        uid: 1,
        minutes: 3 * DAY,
    })
    .unwrap();
    // The schedule moves; baseline 0 keeps the day it was set.
    assert_eq!(scheduled(&doc, on_finish, 0).unwrap().invoice, at(3, 17));
    assert_eq!(base(&doc, on_finish, 0).unwrap().invoice, at(1, 17));
    assert_eq!(base(&doc, on_start, 0).unwrap().invoice, at(1, 8));
    assert_eq!(base(&doc, on_finish, 1), None);

    // Once the task has actual dates they are the invoice dates.
    doc.apply(Command::SetActualStart {
        uid: 2,
        start: Some(at(3, 8)),
    })
    .unwrap();
    doc.apply(Command::SetActualFinish {
        uid: 2,
        finish: Some(at(4, 12)),
    })
    .unwrap();
    let actual = scheduled(&doc, on_finish, 2).unwrap();
    assert_eq!(
        actual,
        PaymentDates {
            invoice: at(4, 12),
            due: at(6, 0),
            actual: true
        }
    );
    assert!(scheduled(&doc, on_start, 0).unwrap().actual);

    // A fixed date is the same for every basis.
    let fixed = PaymentAnchor::Date { at: at(2, 10) };
    assert_eq!(
        scheduled(&doc, fixed, 1),
        base(&doc, fixed, 7).map(|d| PaymentDates { due: at(3, 0), ..d })
    );
    assert_eq!(scheduled(&doc, fixed, 1).unwrap().due, at(3, 0));
}

#[test]
fn net_days_are_calendar_days_across_months_years_and_leap_days() {
    assert_eq!(
        due_date(timestamp(2028, 2, 15, 17, 0), 15),
        timestamp(2028, 3, 1, 0, 0)
    );
    assert_eq!(
        due_date(timestamp(2028, 2, 15, 17, 0), 14),
        timestamp(2028, 2, 29, 0, 0)
    );
    assert_eq!(
        due_date(timestamp(2027, 2, 15, 17, 0), 14),
        timestamp(2027, 3, 1, 0, 0)
    );
    assert_eq!(
        due_date(timestamp(2026, 12, 20, 9, 30), 30),
        timestamp(2027, 1, 19, 0, 0)
    );
    // A due date on a Saturday stays there (D3).
    assert_eq!(due_date(at(0, 17), 5), at(5, 0));
    assert_eq!(due_date(at(0, 17), 0), at(0, 0));
}

#[test]
fn deleting_an_anchored_task_turns_its_payments_into_fixed_dates() {
    let mut doc = doc();
    let settings = CashFlowSettings {
        contract_value: Some(1_000.0),
        opening_balance: 0.0,
        retention: Some(Retention {
            percent: 5.0,
            release: finish(2),
            lag_days: 0,
        }),
    };
    doc.apply(Command::SetCashFlowSettings {
        settings: settings.clone(),
    })
    .unwrap();
    set(
        &mut doc,
        vec![
            input(None, "Mốc 2", PaymentAmount::Percent(50.0), finish(2)),
            input(
                None,
                "Mốc 3",
                PaymentAmount::Percent(50.0),
                PaymentAnchor::Start { task: 3 },
            ),
        ],
    )
    .unwrap();
    let before = doc.project.payments.clone();

    doc.apply(Command::DeleteTask { uid: 2 }).unwrap();
    let detached = &doc.project.payments[0];
    assert_eq!(detached.anchor, PaymentAnchor::Date { at: at(1, 17) });
    assert!(detached.detached);
    assert_eq!(
        doc.project.cash_flow.retention.as_ref().unwrap().release,
        PaymentAnchor::Date { at: at(1, 17) }
    );
    // A payment on another task is left alone (task 3 now starts on day 1).
    assert_eq!(doc.project.payments[1], before[1]);
    let json = serde_json::to_value(&doc.project.payments).unwrap();
    assert_eq!(json[0]["detached"], true);
    assert!(json[1].get("detached").is_none());

    // Undo is a replay: the anchors come back as they were.
    doc.undo().unwrap();
    assert_eq!(doc.project.payments, before);
    assert_eq!(doc.project.cash_flow, settings);
    doc.redo().unwrap();
    assert!(doc.project.payments[0].detached);

    // Keeping a detached payment's date keeps the mark; giving it a task clears it.
    let keep = PaymentInput {
        id: Some(1),
        name: "Mốc 2".into(),
        amount: PaymentAmount::Percent(50.0),
        anchor: PaymentAnchor::Date { at: at(1, 17) },
        lag_days: 0,
        retained: false,
    };
    set(&mut doc, vec![keep.clone()]).unwrap();
    assert!(doc.project.payments[0].detached);
    set(
        &mut doc,
        vec![PaymentInput {
            anchor: finish(3),
            ..keep
        }],
    )
    .unwrap();
    assert!(!doc.project.payments[0].detached);
}

#[test]
fn deleting_a_group_detaches_payments_on_the_tasks_under_it() {
    let mut doc = doc();
    doc.apply(Command::SetPredecessors {
        uid: 2,
        text: String::new(),
    })
    .unwrap();
    doc.apply(Command::Indent { uid: 2 }).unwrap();
    doc.apply(Command::Indent { uid: 3 }).unwrap();
    set(
        &mut doc,
        vec![
            input(None, "Nhóm", PaymentAmount::Fixed(10.0), finish(1)),
            input(None, "Con", PaymentAmount::Fixed(10.0), finish(3)),
        ],
    )
    .unwrap();
    // Indenting made task 1 a group; its payment follows the group's finish.
    let group_finish = doc.project.task(1).unwrap().finish;
    let child_finish = doc.project.task(3).unwrap().finish;
    doc.apply(Command::DeleteTask { uid: 1 }).unwrap();
    assert!(doc.project.task(3).is_none());
    let anchors: Vec<PaymentAnchor> = doc.project.payments.iter().map(|p| p.anchor).collect();
    assert_eq!(
        anchors,
        [
            PaymentAnchor::Date { at: group_finish },
            PaymentAnchor::Date { at: child_finish }
        ]
    );
    assert!(doc.project.payments.iter().all(|p| p.detached));
}

fn series(count: u32, start: i64) -> RecurrenceSpec {
    RecurrenceSpec {
        name: "Báo cáo".into(),
        duration: 60,
        start,
        pattern: RecurrencePattern::Weekly {
            every: 1,
            weekdays: vec![0],
        },
        end: RecurrenceEnd::Count { count },
        calendar_id: None,
        resource_uids: Vec::new(),
        resources: Vec::new(),
        background: false,
    }
}

#[test]
fn a_series_keeps_payments_on_occurrences_that_survive_regenerating() {
    let mut doc = doc();
    doc.apply(Command::CreateRecurringTask {
        after_uid: Some(3),
        spec: series(3, at(0, 8)),
    })
    .unwrap();
    let summary = doc
        .project
        .tasks
        .iter()
        .find(|t| t.recurrence.is_some())
        .unwrap()
        .uid;
    let occurrences: Vec<u32> = doc
        .project
        .tasks
        .iter()
        .filter(|t| t.series_uid == Some(summary))
        .map(|t| t.uid)
        .collect();
    assert_eq!(occurrences.len(), 3);
    set(
        &mut doc,
        vec![
            input(
                None,
                "Cả chuỗi",
                PaymentAmount::Fixed(10.0),
                finish(summary),
            ),
            input(
                None,
                "Lần 1",
                PaymentAmount::Fixed(10.0),
                finish(occurrences[0]),
            ),
            input(
                None,
                "Lần 3",
                PaymentAmount::Fixed(10.0),
                finish(occurrences[2]),
            ),
        ],
    )
    .unwrap();
    let third_finish = doc.project.task(occurrences[2]).unwrap().finish;

    // Two occurrences now: the first keeps its uid and its payment, the third is gone.
    doc.apply(Command::UpdateRecurringTask {
        uid: summary,
        spec: series(2, at(0, 8)),
    })
    .unwrap();
    let anchors: Vec<(PaymentAnchor, bool)> = doc
        .project
        .payments
        .iter()
        .map(|p| (p.anchor, p.detached))
        .collect();
    assert_eq!(anchors[0], (finish(summary), false));
    assert_eq!(anchors[1], (finish(occurrences[0]), false));
    assert_eq!(anchors[2], (PaymentAnchor::Date { at: third_finish }, true));

    // Deleting the series detaches what was on it.
    doc.apply(Command::DeleteTask { uid: summary }).unwrap();
    assert!(doc.project.payments.iter().all(|p| p.detached));
}

#[test]
fn replay_rebuilds_the_same_payments() {
    let mut doc = doc();
    set(
        &mut doc,
        vec![input(None, "Mốc", PaymentAmount::Fixed(10.0), finish(2))],
    )
    .unwrap();
    doc.apply(Command::SetPaymentReceipts {
        id: 1,
        receipts: vec![Receipt {
            date: at(2, 9),
            amount: 4.0,
        }],
    })
    .unwrap();
    doc.apply(Command::DeleteTask { uid: 2 }).unwrap();
    let replayed = replay(&doc.base, &doc.commands[..doc.cursor]).unwrap();
    assert_eq!(replayed.payments, doc.project.payments);
    assert_eq!(replayed.next_payment_id, doc.project.next_payment_id);
}

/// A fresh directory under the system temp dir, so tests do not share files.
fn tempfile_dir() -> std::path::PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("sheafly-payments-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn files_keep_the_data_and_older_files_open_without_it() {
    let mut doc = doc();
    let settings = CashFlowSettings {
        contract_value: Some(500.0),
        opening_balance: 20.0,
        retention: None,
    };
    doc.apply(Command::SetCashFlowSettings {
        settings: settings.clone(),
    })
    .unwrap();
    let mut payment = input(None, "Mốc", PaymentAmount::Percent(40.0), finish(3));
    payment.lag_days = 30;
    payment.retained = true;
    set(&mut doc, vec![payment]).unwrap();
    doc.apply(Command::SetPaymentReceipts {
        id: 1,
        receipts: vec![Receipt {
            date: at(5, 9),
            amount: 200.0,
        }],
    })
    .unwrap();
    let dir = tempfile_dir();
    let path = dir.join("payments.sheafly");
    let path = path.to_str().unwrap();
    save(&doc, path).unwrap();
    let mut loaded = load(path).unwrap();
    assert_eq!(loaded.project.cash_flow, settings);
    assert_eq!(loaded.project.payments, doc.project.payments);
    assert_eq!(loaded.project.next_payment_id, 2);
    // The log replays: undo in the opened file takes the receipt away.
    loaded.undo().unwrap();
    assert!(loaded.project.payments[0].received.is_empty());

    // A project written before payments existed reads with none, and the next id goes past
    // any payment a file holds.
    let mut value = serde_json::to_value(&doc.project).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("cash_flow");
    object.remove("payments");
    object.remove("next_payment_id");
    let older: Project = serde_json::from_value(value).unwrap();
    assert_eq!(older.cash_flow, CashFlowSettings::default());
    assert!(older.payments.is_empty());
    let mut value = serde_json::to_value(&doc.project).unwrap();
    value.as_object_mut().unwrap().remove("next_payment_id");
    let mut without_next = Document::from_project(serde_json::from_value(value).unwrap());
    set(
        &mut without_next,
        vec![
            input(Some(1), "Mốc", PaymentAmount::Fixed(1.0), finish(3)),
            input(None, "Mới", PaymentAmount::Fixed(1.0), finish(3)),
        ],
    )
    .unwrap();
    assert_eq!(ids(&without_next), [1, 2]);
}

#[test]
fn the_json_shape_is_what_the_frontend_mirrors() {
    let mut doc = doc();
    set(
        &mut doc,
        vec![
            input(
                None,
                "A",
                PaymentAmount::Fixed(100.0),
                PaymentAnchor::Start { task: 1 },
            ),
            input(
                None,
                "B",
                PaymentAmount::Percent(30.0),
                PaymentAnchor::Date { at: at(0, 8) },
            ),
        ],
    )
    .unwrap();
    let json = serde_json::to_value(&doc.project).unwrap();
    assert_eq!(
        json["payments"][0]["amount"],
        serde_json::json!({ "fixed": 100.0 })
    );
    assert_eq!(
        json["payments"][0]["anchor"],
        serde_json::json!({ "type": "start", "task": 1 })
    );
    assert_eq!(
        json["payments"][1]["amount"],
        serde_json::json!({ "percent": 30.0 })
    );
    assert_eq!(json["payments"][1]["anchor"]["type"], "date");
    // Written even when empty, so a patch can clear it.
    assert_eq!(
        json["cash_flow"],
        serde_json::json!({ "contract_value": null, "opening_balance": 0.0, "retention": null })
    );
    let command: Command = serde_json::from_value(serde_json::json!({
        "type": "set_payments",
        "payments": [{ "name": "C", "amount": { "fixed": 5 }, "anchor": { "type": "finish", "task": 2 } }]
    }))
    .unwrap();
    doc.apply(command).unwrap();
    assert_eq!(doc.project.payments[0].lag_days, 0);
}

#[test]
fn mspdi_leaves_the_payments_out() {
    let mut doc = doc();
    doc.apply(Command::SetCashFlowSettings {
        settings: CashFlowSettings {
            contract_value: Some(900.0),
            ..Default::default()
        },
    })
    .unwrap();
    set(
        &mut doc,
        vec![input(
            None,
            "Khoản thu theo mốc",
            PaymentAmount::Fixed(100.0),
            finish(2),
        )],
    )
    .unwrap();
    let xml = write_mspdi(&doc.project).unwrap();
    assert!(!xml.contains("Khoản thu theo mốc"));
    let imported = read_mspdi(&xml).unwrap();
    assert!(imported.payments.is_empty());
    assert_eq!(imported.cash_flow, CashFlowSettings::default());
}

#[test]
fn copying_tasks_does_not_carry_payments() {
    let mut doc = doc();
    set(
        &mut doc,
        vec![input(None, "Mốc", PaymentAmount::Fixed(100.0), finish(2))],
    )
    .unwrap();
    let before = doc.project.payments.clone();
    let copied = copy_tasks(&doc.project, &[2]);
    doc.apply(Command::PasteTasks {
        after_uid: Some(3),
        clipboard: copied,
    })
    .unwrap();
    assert_eq!(doc.project.tasks.len(), 4);
    assert_eq!(doc.project.payments, before);
}
