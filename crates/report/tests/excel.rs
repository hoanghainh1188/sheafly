//! The Excel workbook (#168), read back with calamine: the numbers in each sheet must be
//! the engine's, and the formulas must carry their results.

use std::io::Cursor;

use calamine::{Data, DataType, Reader, Xlsx};
use chrono::NaiveDate;
use sheafly_engine::command::Command;
use sheafly_engine::load_map::{load_map, Bucket, LoadGroup};
use sheafly_engine::model::ResourceKind;
use sheafly_engine::skills::{AssignmentRule, RuleTarget};
use sheafly_engine::timeutil::timestamp;
use sheafly_engine::{Document, Lang};
use sheafly_report::excel_workbook;

const DAY: i64 = 480;

type Sheet = Vec<Vec<Data>>;

fn sheets(doc: &Document, lang: Lang) -> Vec<(String, Sheet)> {
    let bytes = excel_workbook(&doc.project, lang).expect("export");
    let mut workbook: Xlsx<_> = Xlsx::new(Cursor::new(bytes)).expect("a workbook");
    let names = workbook.sheet_names().to_vec();
    names
        .into_iter()
        .map(|name| {
            let range = workbook.worksheet_range(&name).expect("a sheet");
            (name, range.rows().map(|row| row.to_vec()).collect())
        })
        .collect()
}

fn sheet<'a>(all: &'a [(String, Sheet)], name: &str) -> &'a Sheet {
    &all.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("no sheet {name}")).1
}

fn text(cell: &Data) -> String {
    cell.to_string()
}

fn number(cell: &Data) -> f64 {
    cell.as_f64().unwrap_or_else(|| panic!("not a number: {cell:?}"))
}

fn column(sheet: &Sheet, header: &str) -> usize {
    sheet[0].iter().position(|c| text(c) == header).unwrap_or_else(|| panic!("no column {header}"))
}

/// A group "Phase" with one leaf of ten working days from Monday 2026-01-26, so the work
/// falls half in January and half in February, and Alice (Backend) on it.
fn project() -> (Document, u32, u32) {
    let mut doc = Document::new_in(Lang::En);
    doc.apply(Command::SetProjectStart { start: timestamp(2026, 1, 26, 8, 0) }).unwrap();
    doc.apply(Command::AddResource { name: "Alice".into(), kind: ResourceKind::Person }).unwrap();
    let alice = doc.project.resources.last().unwrap().uid;
    doc.apply(Command::SetResourceGroup { uid: alice, group: "Backend".into() }).unwrap();
    let phase = doc.project.tasks[0].uid;
    doc.apply(Command::AddTask { name: "Build".into(), after_uid: Some(phase) }).unwrap();
    let leaf = doc.project.tasks.last().unwrap().uid;
    doc.apply(Command::Indent { uid: leaf }).unwrap();
    doc.apply(Command::SetDuration { uid: leaf, minutes: 10 * DAY }).unwrap();
    doc.apply(Command::AssignResource { task_uid: leaf, resource_uid: alice, units: 1.0 }).unwrap();
    (doc, alice, leaf)
}

#[test]
fn time_phased_work_follows_the_engine_by_month() {
    let (doc, _, _) = project();
    let all = sheets(&doc, Lang::En);
    let phased = sheet(&all, "Time-Phased Work");
    let jan = column(phased, "2026-01");
    let feb = column(phased, "2026-02");
    let total = column(phased, "Total (h)");
    // Rows: header, the group, the leaf, the totals.
    let (group, leaf, totals) = (&phased[1], &phased[2], &phased[3]);
    assert_eq!(text(&leaf[1]), "Build");
    assert_eq!((number(&leaf[jan]), number(&leaf[feb])), (40.0, 40.0));
    assert_eq!((number(&group[jan]), number(&group[feb])), (40.0, 40.0), "a group sums its leaves");
    assert_eq!(number(&leaf[total]), 80.0, "the row SUM carries its result");
    // The bottom totals count leaves only, not the group again.
    assert_eq!(text(&totals[1]), "Total (h)");
    assert_eq!((number(&totals[jan]), number(&totals[feb]), number(&totals[total])), (40.0, 40.0, 80.0));
}

#[test]
fn holidays_move_work_between_months_as_the_engine_does() {
    let (mut doc, _, _) = project();
    // Friday 2026-01-30 off: the leaf runs one more day in February.
    doc.apply(Command::AddHoliday { date: "2026-01-30".into() }).unwrap();
    let all = sheets(&doc, Lang::En);
    let phased = sheet(&all, "Time-Phased Work");
    let leaf = &phased[2];
    assert_eq!((number(&leaf[column(phased, "2026-01")]), number(&leaf[column(phased, "2026-02")])), (32.0, 48.0));
}

#[test]
fn resource_loading_holds_numbers_from_the_load_map() {
    let (doc, _, _) = project();
    let all = sheets(&doc, Lang::En);
    let loading = sheet(&all, "Resource Loading");
    let map = load_map(
        &doc.project,
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2026, 2, 28).unwrap(),
        Bucket::Month,
        LoadGroup::Person,
        None,
    )
    .unwrap();
    let cells = &map.rows[0].cells;
    let (work, capacity) = (&loading[1], &loading[2]);
    assert_eq!((text(&work[0]), text(&work[1]), text(&work[2])), ("Alice".into(), "Backend".into(), "Work (h)".into()));
    assert_eq!(text(&capacity[2]), "Capacity (h)");
    for (month, cell) in cells.iter().enumerate() {
        assert_eq!(number(&work[3 + month]), cell.work as f64 / 60.0);
        assert_eq!(number(&capacity[3 + month]), cell.capacity as f64 / 60.0);
    }
    let total = column(loading, "Total (h)");
    assert_eq!(number(&work[total]), 80.0);
}

#[test]
fn the_matrix_names_roles_and_rule_targets() {
    let (mut doc, alice, _) = project();
    doc.apply(Command::AddResource { name: "Bob".into(), kind: ResourceKind::Person }).unwrap();
    let bob = doc.project.resources.last().unwrap().uid;
    let last = doc.project.tasks.last().unwrap().uid;
    doc.apply(Command::AddTask { name: "Review".into(), after_uid: Some(last) }).unwrap();
    doc.apply(Command::SetAssignmentRules {
        rules: vec![AssignmentRule { conditions: vec![], target: RuleTarget::Resource { uid: bob }, units: 1.0 }],
    })
    .unwrap();
    let all = sheets(&doc, Lang::En);
    let matrix = sheet(&all, "Assignment Matrix");
    let headers: Vec<String> = matrix[0].iter().map(text).collect();
    assert!(headers.contains(&"Alice\n(Backend)".to_string()), "{headers:?}");
    assert!(headers.contains(&"Bob".to_string()), "no empty role in the header: {headers:?}");
    let rule = column(matrix, "Unassigned (Rule)");
    let review = matrix.iter().find(|row| text(&row[1]) == "Review").expect("the unassigned task");
    assert_eq!(text(&review[rule]), "Bob");
    let alice_col = headers.iter().position(|h| h.starts_with("Alice")).unwrap();
    let build = matrix.iter().find(|row| text(&row[1]) == "Build").unwrap();
    assert_eq!(number(&build[alice_col]), 1.0);
    let _ = alice;
}

#[test]
fn both_languages_export_four_sheets() {
    let doc = Document::new_default();
    let vi: Vec<String> = sheets(&doc, Lang::Vi).into_iter().map(|(n, _)| n).collect();
    assert_eq!(vi, ["Kế hoạch & Công việc", "Phân bổ công", "Ma trận phân công", "Tải nguồn lực"]);
    let en: Vec<String> = sheets(&doc, Lang::En).into_iter().map(|(n, _)| n).collect();
    assert_eq!(en, ["Tasks & Schedule", "Time-Phased Work", "Assignment Matrix", "Resource Loading"]);
}

#[test]
fn the_tasks_sheet_keeps_dates_effort_and_baseline() {
    let (mut doc, _, leaf) = project();
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();
    let all = sheets(&doc, Lang::En);
    let tasks = sheet(&all, "Tasks & Schedule");
    let row = &tasks[2];
    assert_eq!(text(&row[1]), "Build");
    assert_eq!(number(&row[column(tasks, "Duration (Days)")]), 10.0);
    assert_eq!(number(&row[column(tasks, "Work (Hours)")]), 80.0);
    assert_eq!(text(&row[column(tasks, "Resources")]), "Alice");
    let start = row[column(tasks, "Start")].as_datetime().expect("a date");
    assert_eq!(start.to_string(), "2026-01-26 08:00:00");
    let baseline = row[column(tasks, "Baseline Finish")].as_datetime().expect("a baseline date");
    let task = doc.project.tasks.iter().find(|t| t.uid == leaf).unwrap();
    assert_eq!(baseline.and_utc().timestamp_millis(), task.finish);
}

#[test]
fn resource_loading_highlights_overallocated_months() {
    let (mut doc, alice, leaf) = project();
    // Add a second task for Alice in parallel on Monday 2026-01-26, so she is overloaded in January.
    doc.apply(Command::AddTask {
        name: "Parallel".into(),
        after_uid: Some(leaf),
    })
    .unwrap();
    let parallel = doc.project.tasks.last().unwrap().uid;
    doc.apply(Command::SetDuration {
        uid: parallel,
        minutes: 5 * DAY,
    })
    .unwrap();
    doc.apply(Command::AssignResource {
        task_uid: parallel,
        resource_uid: alice,
        units: 1.0,
    })
    .unwrap();
    assert!(
        !doc.project.overallocations.is_empty(),
        "the engine must flag overallocations"
    );

    let bytes = excel_workbook(&doc.project, Lang::En).expect("export");
    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes)).expect("valid zip");

    // 1. Verify xl/styles.xml defines the red overallocation style (fill FEE2E2, text B91C1C).
    let mut styles_xml = String::new();
    std::io::Read::read_to_string(
        &mut zip.by_name("xl/styles.xml").expect("styles.xml"),
        &mut styles_xml,
    )
    .unwrap();
    assert!(
        styles_xml.contains("FEE2E2"),
        "styles.xml must define the red overallocation background fill"
    );
    assert!(
        styles_xml.contains("B91C1C"),
        "styles.xml must define the red overallocation font color"
    );

    // 2. Locate which cellXfs index uses the overallocation fill.
    // In OpenXML, <fills> lists <fill> items. Find the 0-based fillId with FEE2E2.
    let fill_index = styles_xml
        .split("<fill>")
        .skip(1)
        .position(|f| f.contains("FEE2E2"))
        .expect("must find fill with FEE2E2");

    let expected_fill_attr = format!("fillId=\"{fill_index}\"");
    let cell_xfs_xml = styles_xml
        .split("<cellXfs")
        .nth(1)
        .and_then(|s| s.split("</cellXfs>").next())
        .expect("must find <cellXfs>");

    let over_style_id = cell_xfs_xml
        .split("<xf ")
        .skip(1)
        .position(|xf| xf.contains(&expected_fill_attr))
        .expect("must find cellXfs with overallocation fillId");

    // 3. Inspect xl/worksheets/sheet4.xml (Resource Loading sheet).
    let mut sheet_xml = String::new();
    std::io::Read::read_to_string(
        &mut zip.by_name("xl/worksheets/sheet4.xml").expect("sheet4.xml"),
        &mut sheet_xml,
    )
    .unwrap();

    // Alice's Work is on row 2, columns: A2 (Alice), B2 (Backend), C2 (Work (h)), D2 (2026-01), E2 (2026-02), F2 (Total).
    // The overloaded month cell D2 must reference the overallocated style id.
    let target_cell = format!("r=\"D2\" s=\"{over_style_id}\"");
    assert!(
        sheet_xml.contains(&target_cell),
        "January work cell D2 must have the overallocated style {over_style_id}: {sheet_xml}"
    );

    // February work cell E2 (not overloaded) must NOT have the overallocated style.
    let feb_work_cell = format!("r=\"E2\" s=\"{over_style_id}\"");
    assert!(
        !sheet_xml.contains(&feb_work_cell),
        "February work cell E2 must not have the overallocated style"
    );

    // Alice's Capacity row (row 3, cell D3) must NOT have the overallocated style.
    let capacity_cell = format!("r=\"D3\" s=\"{over_style_id}\"");
    assert!(
        !sheet_xml.contains(&capacity_cell),
        "Capacity row must not have the overallocated style"
    );
}
