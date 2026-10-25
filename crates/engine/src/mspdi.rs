use std::collections::BTreeMap;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::availability::AvailabilityPeriod;
use crate::calendar::{Calendar, CalendarBase, TimeRange, WorkWeek};
use crate::error::EngineError;
use crate::model::{
    Accrual, Assignment, AssignmentBaseline, Baseline, ConstraintType, DepKind, Dependency, Project, Resource,
    ResourceKind, Task, TaskType,
};
use crate::mspdi_availability::{self, PeriodBuilder};
use crate::mspdi_baseline::{self, BaselineReader};
use crate::mspdi_calendar_days;
use crate::mspdi_cost;
use crate::mspdi_fields::{self, FieldReader};
use crate::mspdi_range::{self, RangeReader};
use crate::mspdi_skills::{self, SkillReader};
use crate::mspdi_report::{ImportNote, ImportReport};
use crate::mspdi_workweeks::{self, WorkWeekBuilder};
use crate::mspdi_progress::{self, FileProgress, TaskProgress, TimephasedActual};
use crate::schedule::schedule;
use crate::split::Split;
use crate::timeutil::{format_duration_iso, format_iso, parse_duration_iso, parse_iso};

/// Write MSPDI XML for tasks, dependencies, calendars, resources, assignments, and baselines.
pub fn write_mspdi(project: &Project) -> Result<String, EngineError> {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Project xmlns=\"http://schemas.microsoft.com/project\">\n",
    );
    el(&mut xml, "Name", &escape(&project.name));
    el(&mut xml, "StartDate", &format_iso(project.start));
    if let Some(finish) = project.tasks.iter().map(|task| task.finish).max() {
        el(&mut xml, "FinishDate", &format_iso(finish));
    }
    el(&mut xml, "CalendarUID", &project.calendar_id.to_string());
    mspdi_cost::write_editable(&mut xml, project);
    if let Some(status) = project.status_date {
        el(&mut xml, "StatusDate", &format_iso(status));
    }
    mspdi_fields::write_definitions(&mut xml, project);
    xml.push_str("<Calendars>\n");
    for calendar in &project.calendars {
        write_calendar(&mut xml, calendar);
    }
    xml.push_str("</Calendars>\n<Tasks>\n");
    for task in &project.tasks {
        write_task(&mut xml, project, task);
    }
    xml.push_str("</Tasks>\n<Resources>\n");
    for resource in &project.resources {
        write_resource(&mut xml, project, resource);
    }
    xml.push_str("</Resources>\n<Assignments>\n");
    for assignment in &project.assignments {
        write_assignment(&mut xml, project, assignment);
    }
    write_unassigned_splits(&mut xml, project);
    xml.push_str("</Assignments>\n</Project>\n");
    Ok(xml)
}

/// Microsoft Project keeps split segments as assignment timephased work. A split task
/// with no resource gets an assignment to the "unassigned" resource (UID -65535).
fn write_unassigned_splits(xml: &mut String, project: &Project) {
    let mut next_uid = project
        .assignments
        .iter()
        .map(|assignment| assignment.uid)
        .max()
        .unwrap_or(0)
        + 1;
    for task in &project.tasks {
        if task.summary
            || task.segments.len() < 2
            || project
                .assignments
                .iter()
                .any(|assignment| assignment.task_uid == task.uid)
        {
            continue;
        }
        xml.push_str("<Assignment>\n");
        el(xml, "UID", &next_uid.to_string());
        el(xml, "TaskUID", &task.uid.to_string());
        el(xml, "ResourceUID", "-65535");
        // Microsoft Project has no productivity factor: units at the rate give its dates.
        el(xml, "Units", &format_number(project.focus_factor));
        el(xml, "Work", &format_duration_iso(task.work));
        write_timephased(xml, project, task, next_uid, task.work);
        mspdi_progress::write_actual_timephased(xml, project, task, next_uid, task.actual_work);
        xml.push_str("</Assignment>\n");
        next_uid += 1;
    }
}

/// Planned work per segment (type 1), with a zero-work period for each gap.
fn write_timephased(
    xml: &mut String,
    project: &Project,
    task: &Task,
    assignment_uid: u32,
    work: i64,
) {
    if task.segments.len() < 2 {
        return;
    }
    let cal = project.task_calendar(task);
    let minutes: Vec<i64> = task
        .segments
        .iter()
        .map(|segment| cal.working_between(segment.start, segment.finish).max(0))
        .collect();
    let total: i64 = minutes.iter().sum::<i64>().max(1);
    let mut assigned = 0i64;
    for (index, segment) in task.segments.iter().enumerate() {
        let last = index + 1 == task.segments.len();
        let value = if last {
            (work - assigned).max(0)
        } else {
            (work as f64 * minutes[index] as f64 / total as f64).round() as i64
        };
        assigned += value;
        write_period(xml, assignment_uid, segment.start, segment.finish, value);
        if let Some(next) = task.segments.get(index + 1) {
            write_period(xml, assignment_uid, segment.finish, next.start, 0);
        }
    }
}

fn write_period(xml: &mut String, assignment_uid: u32, start: i64, finish: i64, minutes: i64) {
    xml.push_str("<TimephasedData>\n");
    el(xml, "Type", "1");
    el(xml, "UID", &assignment_uid.to_string());
    el(xml, "Start", &format_iso(start));
    el(xml, "Finish", &format_iso(finish));
    el(xml, "Unit", "2");
    el(xml, "Value", &format_duration_iso(minutes));
    xml.push_str("</TimephasedData>\n");
}

/// Rebuild splits from timephased work: gaps are working time with no work between pieces.
/// Planned work (type 1) shapes the splits when present; actual work (type 2) only
/// when there is no planned data, since a day with no actual work is not a split.
fn splits_from_periods(cal: &Calendar, periods: &[Period]) -> Vec<Split> {
    let planned = periods.iter().any(|period| period.kind == 1);
    let mut work: Vec<(i64, i64)> = periods
        .iter()
        .filter(|period| !planned || period.kind == 1)
        .filter(|period| period.minutes > 0 && period.finish > period.start)
        .map(|period| (period.start, period.finish))
        .collect();
    work.sort_unstable();
    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (start, finish) in work {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 || cal.working_between(last.1, start) <= 0 {
                last.1 = last.1.max(finish);
                continue;
            }
        }
        merged.push((start, finish));
    }
    let mut splits = Vec::new();
    let mut done = 0i64;
    for pair in merged.windows(2) {
        done += cal.working_between(pair[0].0, pair[0].1).max(0);
        splits.push(Split {
            offset: done,
            gap: cal.working_between(pair[0].1, pair[1].0).max(0),
            leveling: false,
        });
    }
    splits
}

pub fn read_mspdi(xml: &str) -> Result<Project, EngineError> {
    read_mspdi_with_report(xml).map(|(project, _)| project)
}

/// Read MSPDI and say what the engine read differently from the file.
pub fn read_mspdi_with_report(xml: &str) -> Result<(Project, ImportReport), EngineError> {
    let mut notes: Vec<ImportNote> = Vec::new();
    let mut file_dates: BTreeMap<u32, (i64, i64)> = BTreeMap::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<String> = Vec::new();
    let mut text = String::new();
    let mut project = Project::new("Dự án", 0);
    project.calendars.clear();
    project.tasks.clear();
    let mut calendar: Option<CalendarBuilder> = None;
    let mut weekday: Option<WeekdayBuilder> = None;
    let mut working: Option<TimeRange> = None;
    let mut exception: Option<ExceptionBuilder> = None;
    let mut work_week: Option<WorkWeekBuilder> = None;
    let mut available: Option<PeriodBuilder> = None;
    let mut task: Option<TaskBuilder> = None;
    let mut link: Option<LinkBuilder> = None;
    let mut baseline: Option<Baseline> = None;
    let mut resource: Option<ResourceBuilder> = None;
    let mut assignment: Option<AssignmentBuilder> = None;
    let mut period: Option<Period> = None;
    let mut timephased: BTreeMap<u32, Vec<Period>> = BTreeMap::new();
    let mut file_progress: BTreeMap<u32, FileProgress> = BTreeMap::new();
    let mut actuals: BTreeMap<u32, TimephasedActual> = BTreeMap::new();
    let mut fields = FieldReader::default();
    let mut skills = SkillReader::default();
    let mut effort_ranges = RangeReader::default();
    let mut baseline_pieces = BaselineReader::default();

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                stack.push(local_name(event.name().as_ref()));
                text.clear();
                if let Some(name) = stack.last() {
                    let parent = stack.iter().rev().nth(1).map(String::as_str);
                    fields.start(name, parent);
                    effort_ranges.start(name, parent);
                    skills.start(name, parent);
                }
                match stack.last().map(String::as_str) {
                    Some("Calendar") => calendar = Some(CalendarBuilder::default()),
                    Some("WeekDay") => weekday = Some(WeekdayBuilder::default()),
                    Some("WorkingTime") => {
                        working = Some(TimeRange {
                            start_minute: 0,
                            end_minute: 0,
                        })
                    }
                    Some("Exception") => exception = Some(ExceptionBuilder::default()),
                    Some("WorkWeek") => work_week = Some(WorkWeekBuilder::default()),
                    Some("Task") => task = Some(TaskBuilder::default()),
                    Some("PredecessorLink") => link = Some(LinkBuilder::default()),
                    Some("Baseline") => {
                        baseline = Some(Baseline {
                            number: 0,
                            start: 0,
                            finish: 0,
                            work: 0,
                            cost: 0.0,
                            segments: Vec::new(),
                            fixed_cost: None,
                        })
                    }
                    Some("Resource") => resource = Some(ResourceBuilder::default()),
                    Some("AvailabilityPeriod") => available = Some(PeriodBuilder::default()),
                    Some("Assignment") => assignment = Some(AssignmentBuilder::default()),
                    Some("TimephasedData") => period = Some(Period::default()),
                    _ => {}
                }
            }
            Ok(Event::Empty(event)) => {
                let _ = local_name(event.name().as_ref());
            }
            Ok(Event::Text(event)) => {
                text = event.unescape().unwrap_or_default().into_owned();
            }
            Ok(Event::End(event)) => {
                let name = local_name(event.name().as_ref());
                let parent = stack.iter().rev().nth(1).map(String::as_str);
                if parent == Some("TimephasedData") {
                    if let Some(item) = period.as_mut() {
                        let value = text.trim();
                        match name.as_str() {
                            "Type" => item.kind = value.parse().unwrap_or(0),
                            "Start" => item.start = parse_iso(value).unwrap_or(0),
                            "Finish" => item.finish = parse_iso(value).unwrap_or(0),
                            "Value" => item.minutes = parse_duration_iso(value).unwrap_or(0),
                            _ => {}
                        }
                    }
                }
                if let Some(item) = work_week.as_mut() {
                    item.read(parent, &name, text.trim());
                }
                if let Some(item) = available.as_mut() {
                    item.read(parent, &name, &text);
                }
                fields.text(&name, parent, &text);
                fields.end(&name, parent);
                effort_ranges.text(&name, parent, &text);
                effort_ranges.end(&name, parent);
                skills.text(&name, parent, &text);
                skills.end(&name, parent);
                apply_text(
                    &name,
                    parent,
                    &text,
                    &mut project,
                    Open {
                        calendar: calendar.as_mut(),
                        weekday: weekday.as_mut(),
                        working: working.as_mut(),
                        exception: exception.as_mut(),
                        task: task.as_mut(),
                        link: link.as_mut(),
                        baseline: baseline.as_mut(),
                        resource: resource.as_mut(),
                        assignment: assignment.as_mut(),
                    },
                );
                match name.as_str() {
                    "WorkingTime" => {
                        if let Some(range) = working.take() {
                            if range.end_minute > range.start_minute {
                                if let Some(day) = weekday.as_mut() {
                                    day.ranges.push(range);
                                } else if let Some(item) = exception.as_mut() {
                                    item.ranges.push(range);
                                }
                            }
                        }
                    }
                    "WeekDay" => {
                        if let (Some(cal), Some(day)) = (calendar.as_mut(), weekday.take()) {
                            if day.day_type == Some(0) && work_week.is_none() {
                                // DayType 0 is an exception, written like an `<Exception>`.
                                if !mspdi_calendar_days::add_exception(
                                    &mut cal.exceptions,
                                    day.from.as_deref(),
                                    day.to.as_deref(),
                                    day.working,
                                    &day.ranges,
                                ) {
                                    cal.exceptions_without_dates += 1;
                                }
                            } else if let Some(index) = day.day_type.and_then(day_type_index) {
                                let ranges = if day.working { day.ranges } else { Vec::new() };
                                match work_week.as_mut() {
                                    Some(item) => item.week[index] = Some(ranges),
                                    None => {
                                        cal.week[index] = ranges;
                                        cal.listed[index] = true;
                                    }
                                }
                            }
                        }
                    }
                    "WorkWeek" => {
                        if let (Some(cal), Some(item)) = (calendar.as_mut(), work_week.take()) {
                            match item.finish(&cal.week) {
                                Some(week) => cal.work_weeks.push(week),
                                None => cal.weeks_without_dates += 1,
                            }
                        }
                    }
                    "Exception" => {
                        if let (Some(cal), Some(item)) = (calendar.as_mut(), exception.take()) {
                            mspdi_calendar_days::add_exception(
                                &mut cal.exceptions,
                                item.from.as_deref(),
                                item.to.as_deref(),
                                item.working,
                                &item.ranges,
                            );
                        }
                    }
                    "Calendar" => {
                        if let Some(builder) = calendar.take() {
                            project.calendars.push(builder.finish(&mut notes));
                        }
                    }
                    "PredecessorLink" => {
                        if let (Some(task), Some(link)) = (task.as_mut(), link.take()) {
                            task.links.push(link);
                        }
                    }
                    "Baseline" => {
                        if let Some(b) = baseline.take() {
                            if let Some(task) = task.as_mut() {
                                task.baselines.push(b);
                            } else if let Some(assignment) = assignment.as_mut() {
                                assignment.baselines.push(AssignmentBaseline {
                                    number: b.number,
                                    work: b.work,
                                    cost: b.cost,
                                });
                            }
                        }
                    }
                    "Task" => {
                        if let Some(builder) = task.take() {
                            if !builder.is_null && builder.uid != 0 {
                                fields.end_task(builder.uid);
                                effort_ranges.end_task(builder.uid);
                                file_progress.insert(
                                    builder.uid,
                                    FileProgress {
                                        progress: builder.progress.clone(),
                                        duration: builder.duration,
                                        remaining_duration: builder.remaining_duration,
                                        actual_start: builder.actual_start,
                                        actual_finish: builder.actual_finish,
                                        start: builder.start,
                                        percent: builder.percent,
                                        percent_given: builder.percent_given,
                                    },
                                );
                                if let (Some(start), Some(finish)) = (builder.start, builder.finish) {
                                    file_dates.insert(builder.uid, (start, finish));
                                }
                                let mut next_dep = project.next_dependency_uid;
                                let (parsed, deps) = builder.into_task_and_links(&mut next_dep);
                                project.next_dependency_uid = next_dep;
                                project.tasks.push(parsed);
                                project.dependencies.extend(deps);
                            }
                        }
                    }
                    "AvailabilityPeriod" => {
                        if let (Some(owner), Some(item)) = (resource.as_mut(), available.take()) {
                            owner.periods.push(item.finish());
                        }
                    }
                    "Resource" => {
                        if let Some(builder) = resource.take() {
                            if builder.uid != 0 {
                                skills.end_resource(builder.uid);
                                project.resources.push(builder.finish(&mut notes));
                            }
                        }
                    }
                    "TimephasedData" => {
                        // Types 1 and 2 are remaining and actual assignment work.
                        match (assignment.as_mut(), task.as_ref(), period.take()) {
                            (Some(owner), _, Some(item)) => {
                                if matches!(item.kind, 1 | 2) {
                                    owner.periods.push(item);
                                }
                            }
                            // A task's own timephased data: the pieces of a split baseline.
                            (None, Some(owner), Some(item)) => baseline_pieces.add(
                                owner.uid,
                                item.kind,
                                item.start,
                                item.finish,
                                item.minutes,
                            ),
                            _ => {}
                        }
                    }
                    "Assignment" => {
                        if let Some(mut builder) = assignment.take() {
                            let periods = std::mem::take(&mut builder.periods);
                            for item in periods.iter().filter(|item| item.kind == 2) {
                                actuals
                                    .entry(builder.task_uid)
                                    .or_default()
                                    .add(item.start, item.minutes);
                            }
                            if builder.task_uid != 0 && !periods.is_empty() {
                                timephased
                                    .entry(builder.task_uid)
                                    .or_default()
                                    .extend(periods);
                            }
                            if builder.task_uid != 0 && builder.resource_uid != 0 {
                                project.assignments.push(builder.finish());
                            }
                        }
                    }
                    _ => {}
                }
                if stack.last().map(String::as_str) == Some(name.as_str()) {
                    stack.pop();
                }
                text.clear();
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(EngineError::Xml(err.to_string())),
            _ => {}
        }
    }

    drop_repeated_emails(&mut project);
    fields.finish(&mut project, &mut notes, &effort_ranges.taken());
    effort_ranges.finish(&mut project);
    skills.finish(&mut project);
    if project.calendars.is_empty() {
        project.calendars.push(Calendar::standard());
    }
    if project.calendar_id == 0 {
        project.calendar_id = project.calendars[0].id;
    }
    if project.calendar_by_id(project.calendar_id).is_none() {
        project.calendar_id = project.calendars[0].id;
    }
    // Based calendars hold only their own part until merged with their base.
    crate::calendar_base::refresh(&mut project);
    project.next_task_uid = project.tasks.iter().map(|task| task.uid).max().unwrap_or(0) + 1;
    project.next_resource_uid = project
        .resources
        .iter()
        .map(|resource| resource.uid)
        .max()
        .unwrap_or(0)
        + 1;
    project.next_assignment_uid = project
        .assignments
        .iter()
        .map(|assignment| assignment.uid)
        .max()
        .unwrap_or(0)
        + 1;
    project.next_dependency_uid = project
        .dependencies
        .iter()
        .map(|dep| dep.uid)
        .max()
        .unwrap_or(0)
        + 1;
    project.next_calendar_uid = project
        .calendars
        .iter()
        .map(|calendar| calendar.id)
        .max()
        .unwrap_or(0)
        + 1;
    for task in &mut project.tasks {
        let work: i64 = project
            .assignments
            .iter()
            .filter(|assignment| assignment.task_uid == task.uid)
            .map(|assignment| assignment.work)
            .sum();
        let units: f64 = project
            .assignments
            .iter()
            .filter(|assignment| assignment.task_uid == task.uid)
            .map(|assignment| assignment.units)
            .sum();
        if work > 0 {
            task.work = work;
        }
        if units > 0.0 {
            task.units = units;
        }
    }
    let splits: Vec<(usize, Vec<Split>)> = project
        .tasks
        .iter()
        .enumerate()
        .filter_map(|(index, task)| {
            let periods = timephased.get(&task.uid)?;
            Some((
                index,
                splits_from_periods(project.task_calendar(task), periods),
            ))
        })
        .collect();
    for (index, task_splits) in splits {
        project.tasks[index].splits = task_splits;
    }
    baseline_pieces.finish(&mut project);
    if !project.editable_actual_costs {
        // The file's actual costs are Project's own calculation, not entries.
        for task in &mut project.tasks {
            task.manual_actual_cost = None;
        }
        for assignment in &mut project.assignments {
            assignment.manual_actual_cost = None;
        }
    }
    for task in &mut project.tasks {
        if let Some(file) = file_progress.get(&task.uid) {
            if let Some((file, used)) = file.apply(task, actuals.get(&task.uid).copied()) {
                notes.push(ImportNote::PercentFromActuals { task: task.name.clone(), file, used });
            }
        }
    }
    project.start = project.calendar().next_work_start(project.start);
    schedule(&mut project)?;
    if keep_real_entries(&mut project) {
        schedule(&mut project)?;
    }
    // A minute either way is rounding in the file, not a move. Largest moves first.
    let mut moved: Vec<(i64, ImportNote)> = project
        .tasks
        .iter()
        .filter(|task| !task.summary)
        .filter_map(|task| {
            let &(start, finish) = file_dates.get(&task.uid)?;
            let shift = (task.start - start).abs().max((task.finish - finish).abs());
            (shift > 60_000).then(|| {
                let note = ImportNote::Moved {
                    task: task.name.clone(),
                    before: (start, finish),
                    after: (task.start, task.finish),
                };
                (shift, note)
            })
        })
        .collect();
    moved.sort_by_key(|(shift, _)| std::cmp::Reverse(*shift));
    notes.extend(moved.into_iter().map(|(_, note)| note));
    Ok((project, ImportReport { notes }))
}

/// The elements being read when a text node arrives; at most a few are open at once.
struct Open<'a> {
    calendar: Option<&'a mut CalendarBuilder>,
    weekday: Option<&'a mut WeekdayBuilder>,
    working: Option<&'a mut TimeRange>,
    exception: Option<&'a mut ExceptionBuilder>,
    task: Option<&'a mut TaskBuilder>,
    link: Option<&'a mut LinkBuilder>,
    baseline: Option<&'a mut Baseline>,
    resource: Option<&'a mut ResourceBuilder>,
    assignment: Option<&'a mut AssignmentBuilder>,
}

fn apply_text(name: &str, parent: Option<&str>, text: &str, project: &mut Project, open: Open<'_>) {
    let Open {
        calendar,
        weekday,
        working,
        exception,
        task,
        link,
        baseline,
        resource,
        assignment,
    } = open;
    let value = text.trim();
    if value.is_empty() {
        return;
    }
    match (parent, name) {
        (Some("Project"), "Name") => project.name = value.to_string(),
        (Some("Project"), "StartDate") => {
            if let Some(ms) = parse_iso(value) {
                project.start = ms;
            }
        }
        (Some("Project"), "CalendarUID") => {
            if let Ok(id) = value.parse() {
                project.calendar_id = id;
            }
        }
        (Some("Project"), "StatusDate") => project.status_date = parse_iso(value),
        (Some("Project"), "EditableActualCosts") => {
            project.editable_actual_costs = value == "1" || value.eq_ignore_ascii_case("true");
        }
        (Some("Calendar"), "UID") => {
            if let Some(calendar) = calendar {
                calendar.id = value.parse().unwrap_or(calendar.id);
            }
        }
        (Some("Calendar"), "BaseCalendarUID") => {
            if let Some(calendar) = calendar {
                calendar.base_uid = value.parse().ok().filter(|uid| *uid != 0);
            }
        }
        (Some("Calendar"), "Name") => {
            if let Some(calendar) = calendar {
                calendar.name = value.to_string();
            }
        }
        (Some("WeekDay"), "DayType") => {
            if let Some(day) = weekday {
                day.day_type = value.parse().ok();
            }
        }
        (Some("WeekDay"), "DayWorking") => {
            if let Some(day) = weekday {
                day.working = value != "0";
            }
        }
        (Some("WorkingTime"), "FromTime") => {
            if let Some(range) = working {
                range.start_minute = parse_clock(value);
            }
        }
        (Some("WorkingTime"), "ToTime") => {
            if let Some(range) = working {
                range.end_minute = parse_clock(value);
            }
        }
        (Some("Exception"), "DayWorking") => {
            if let Some(item) = exception {
                item.working = value != "0";
            }
        }
        (Some("TimePeriod"), "FromDate") => {
            if let Some(item) = exception {
                item.from = Some(value.to_string());
            } else if let Some(day) = weekday {
                day.from = Some(value.to_string());
            }
        }
        (Some("TimePeriod"), "ToDate") => {
            if let Some(item) = exception {
                item.to = Some(value.to_string());
            } else if let Some(day) = weekday {
                day.to = Some(value.to_string());
            }
        }
        (Some("Task"), "UID") => {
            if let Some(task) = task {
                task.uid = value.parse().unwrap_or(0);
            }
        }
        (Some("Task"), "Name") => {
            if let Some(task) = task {
                task.name = value.to_string();
            }
        }
        (Some("Task"), "OutlineLevel") => {
            if let Some(task) = task {
                task.outline_level = value.parse().unwrap_or(1);
            }
        }
        (Some("Task"), "Duration") => {
            if let Some(task) = task {
                task.duration = parse_duration_iso(value).unwrap_or(0);
            }
        }
        (Some("Task"), "Milestone") => {
            if let Some(task) = task {
                task.milestone = value == "1";
            }
        }
        (Some("Task"), "Manual") => {
            if let Some(task) = task {
                task.manual = value == "1";
            }
        }
        (Some("Task"), "Notes") => {
            if let Some(task) = task {
                task.notes = value.to_string();
            }
        }
        (Some("Task"), "Priority") => {
            if let Some(task) = task {
                task.priority = value.parse().unwrap_or(500);
            }
        }
        (Some("Task"), "ConstraintType") => {
            if let Some(task) = task {
                task.constraint = value.parse().ok().and_then(constraint_from_xml);
            }
        }
        (Some("Task"), "ConstraintDate") => {
            if let Some(task) = task {
                task.constraint_date = parse_iso(value);
            }
        }
        (Some("Task"), "CalendarUID") => {
            if let Some(task) = task {
                task.calendar_id = value.parse().ok();
            }
        }
        (Some("Task"), "Type") => {
            if let Some(task) = task {
                task.task_type = match value {
                    "1" => TaskType::FixedDuration,
                    "2" => TaskType::FixedWork,
                    _ => TaskType::FixedUnits,
                };
            }
        }
        (Some("Task"), "EffortDriven") => {
            if let Some(task) = task {
                task.effort_driven = value != "0";
            }
        }
        (Some("Task"), "PercentComplete") => {
            if let Some(task) = task {
                task.percent = value.parse().unwrap_or(0.0);
                task.percent_given = true;
            }
        }
        (Some("Task"), "ActualStart") => {
            if let Some(task) = task {
                task.actual_start = parse_iso(value);
            }
        }
        (Some("Task"), "ActualFinish") => {
            if let Some(task) = task {
                task.actual_finish = parse_iso(value);
            }
        }
        (Some("Task"), "RemainingDuration") => {
            if let Some(task) = task {
                task.remaining_duration = parse_duration_iso(value);
            }
        }
        (Some("Task"), "Start") => {
            if let Some(task) = task {
                task.start = parse_iso(value);
            }
        }
        (Some("Task"), "Finish") => {
            if let Some(task) = task {
                task.finish = parse_iso(value);
            }
        }
        (Some("Task"), "FixedCost") => {
            if let Some(task) = task {
                task.fixed_cost = value.parse().unwrap_or(0.0);
            }
        }
        (Some("Task"), "FixedCostAccrual") => {
            if let Some(task) = task {
                task.accrual = match value {
                    "1" => Accrual::Start,
                    "3" => Accrual::End,
                    _ => Accrual::Prorated,
                };
            }
        }
        (Some("Task"), "LevelingDelay") => {
            if let Some(task) = task {
                let tenths: i64 = value.parse().unwrap_or(0);
                task.leveling_delay = tenths / 10;
            }
        }
        (Some("Task"), "IsNull") => {
            if let Some(task) = task {
                task.is_null = value == "1";
            }
        }
        (Some("PredecessorLink"), "PredecessorUID") => {
            if let Some(link) = link {
                link.predecessor = value.parse().unwrap_or(0);
            }
        }
        (Some("PredecessorLink"), "Type") => {
            if let Some(link) = link {
                link.kind = match value {
                    "0" => DepKind::Ff,
                    "2" => DepKind::Ss,
                    "3" => DepKind::Sf,
                    _ => DepKind::Fs,
                };
            }
        }
        (Some("PredecessorLink"), "LinkLag") => {
            if let Some(link) = link {
                let tenths: i64 = value.parse().unwrap_or(0);
                link.lag = tenths / 10;
            }
        }
        (Some("Baseline"), "Number") => {
            if let Some(baseline) = baseline {
                baseline.number = value.parse().unwrap_or(0);
            }
        }
        (Some("Baseline"), "Start") => {
            if let Some(baseline) = baseline {
                baseline.start = parse_iso(value).unwrap_or(0);
            }
        }
        (Some("Baseline"), "Finish") => {
            if let Some(baseline) = baseline {
                baseline.finish = parse_iso(value).unwrap_or(0);
            }
        }
        (Some("Baseline"), "Work") => {
            if let Some(baseline) = baseline {
                baseline.work = parse_duration_iso(value).unwrap_or(0);
            }
        }
        (Some("Baseline"), "Cost") => {
            if let Some(baseline) = baseline {
                baseline.cost = value.parse().unwrap_or(0.0);
            }
        }
        (Some("Resource"), "UID") => {
            if let Some(resource) = resource {
                resource.uid = value.parse().unwrap_or(0);
            }
        }
        (Some("Resource"), "Name") => {
            if let Some(resource) = resource {
                resource.name = value.to_string();
            }
        }
        (Some("Resource"), "Group") => {
            if let Some(resource) = resource {
                resource.group = value.to_string();
            }
        }
        (Some("Resource"), "EmailAddress") => {
            if let Some(resource) = resource {
                resource.email = value.trim().to_string();
            }
        }
        (Some("Resource"), "MaxUnits") => {
            if let Some(resource) = resource {
                resource.max_units = value.parse().unwrap_or(1.0);
            }
        }
        (Some("Resource"), "StandardRate") => {
            if let Some(resource) = resource {
                resource.standard_rate = value.parse().unwrap_or(0.0);
            }
        }
        (Some("Resource"), "OvertimeRate") => {
            if let Some(resource) = resource {
                resource.overtime_rate = value.parse().unwrap_or(0.0);
            }
        }
        (Some("Resource"), "CostPerUse") => {
            if let Some(resource) = resource {
                resource.cost_per_use = value.parse().unwrap_or(0.0);
            }
        }
        (Some("Resource"), "CalendarUID") => {
            if let Some(resource) = resource {
                resource.calendar_id = value.parse().ok();
            }
        }
        (Some("Assignment"), "UID") => {
            if let Some(assignment) = assignment {
                assignment.uid = value.parse().unwrap_or(0);
            }
        }
        (Some("Assignment"), "TaskUID") => {
            if let Some(assignment) = assignment {
                assignment.task_uid = value.parse().unwrap_or(0);
            }
        }
        (Some("Assignment"), "ResourceUID") => {
            if let Some(assignment) = assignment {
                assignment.resource_uid = value.parse().unwrap_or(0);
            }
        }
        (Some("Assignment"), "Units") => {
            if let Some(assignment) = assignment {
                assignment.units = value.parse().unwrap_or(1.0);
            }
        }
        (Some("Assignment"), "Work") => {
            if let Some(assignment) = assignment {
                assignment.work = parse_duration_iso(value).unwrap_or(0);
            }
        }
        (Some("Assignment"), "ActualCost") => {
            if let Some(assignment) = assignment {
                assignment.actual_cost = value.parse().ok().filter(|cost: &f64| cost.is_finite());
            }
        }
        (Some("Assignment"), "OvertimeWork") => {
            if let Some(assignment) = assignment {
                assignment.overtime_work = parse_duration_iso(value).unwrap_or(0);
            }
        }
        (Some("Task"), field) => {
            if let Some(task) = task {
                task.progress.read(field, value);
            }
        }
        _ => {}
    }
}

fn write_calendar(xml: &mut String, calendar: &Calendar) {
    xml.push_str("<Calendar>\n");
    el(xml, "UID", &calendar.id.to_string());
    el(xml, "Name", &escape(&calendar.name));
    // A based calendar writes only what it sets itself, as Project does.
    let (exceptions, work_weeks) = match &calendar.base {
        Some(own) => {
            el(xml, "IsBaseCalendar", "0");
            el(xml, "BaseCalendarUID", &own.id.to_string());
            xml.push_str("<WeekDays>\n");
            write_some_week_days(xml, &own.week);
            (&own.exceptions, own.work_weeks.as_slice())
        }
        None => {
            el(xml, "IsBaseCalendar", "1");
            xml.push_str("<WeekDays>\n");
            write_week_days(xml, &calendar.week);
            (&calendar.exceptions, calendar.work_weeks.as_slice())
        }
    };
    xml.push_str("</WeekDays>\n<Exceptions>\n");
    for (date, ranges) in exceptions {
        xml.push_str("<Exception>\n<EnteredByOccurrences>0</EnteredByOccurrences>\n<TimePeriod>\n");
        el(xml, "FromDate", &format!("{date}T00:00:00"));
        el(xml, "ToDate", &format!("{date}T23:59:00"));
        xml.push_str("</TimePeriod>\n");
        el(xml, "Name", "Ngoại lệ");
        el(xml, "DayWorking", if ranges.is_empty() { "0" } else { "1" });
        if !ranges.is_empty() {
            xml.push_str("<WorkingTimes>\n");
            for range in ranges {
                xml.push_str("<WorkingTime>\n");
                el(xml, "FromTime", &format_clock(range.start_minute));
                el(xml, "ToTime", &format_clock(range.end_minute));
                xml.push_str("</WorkingTime>\n");
            }
            xml.push_str("</WorkingTimes>\n");
        }
        xml.push_str("</Exception>\n");
    }
    xml.push_str("</Exceptions>\n");
    mspdi_workweeks::write_work_weeks(xml, work_weeks);
    xml.push_str("</Calendar>\n");
}

/// The seven `<WeekDay>` elements of a weekly pattern.
pub(crate) fn write_week_days(xml: &mut String, week: &[Vec<TimeRange>; 7]) {
    let days: [Option<Vec<TimeRange>>; 7] = week.clone().map(Some);
    write_some_week_days(xml, &days);
}

/// `<WeekDay>` elements for the days that are set; the others are left out.
fn write_some_week_days(xml: &mut String, week: &[Option<Vec<TimeRange>>; 7]) {
    for (index, ranges) in week.iter().enumerate() {
        let Some(ranges) = ranges else {
            continue;
        };
        xml.push_str("<WeekDay>\n");
        el(xml, "DayType", &index_to_day_type(index).to_string());
        el(xml, "DayWorking", if ranges.is_empty() { "0" } else { "1" });
        if !ranges.is_empty() {
            xml.push_str("<WorkingTimes>\n");
            for range in ranges {
                xml.push_str("<WorkingTime>\n");
                el(xml, "FromTime", &format_clock(range.start_minute));
                el(xml, "ToTime", &format_clock(range.end_minute));
                xml.push_str("</WorkingTime>\n");
            }
            xml.push_str("</WorkingTimes>\n");
        }
        xml.push_str("</WeekDay>\n");
    }
}

pub(crate) fn escape_text(text: &str) -> String {
    escape(text)
}

fn write_task(xml: &mut String, project: &Project, task: &Task) {
    xml.push_str("<Task>\n");
    el(xml, "UID", &task.uid.to_string());
    el(xml, "ID", &task.id.to_string());
    el(xml, "Name", &escape(&task.name));
    el(xml, "Type", &task_type_xml(task.task_type).to_string());
    el(xml, "IsNull", "0");
    el(xml, "WBS", &escape(&task.wbs));
    el(xml, "OutlineNumber", &escape(&task.wbs));
    el(xml, "OutlineLevel", &task.outline_level.to_string());
    el(xml, "Start", &format_iso(task.start));
    el(xml, "Finish", &format_iso(task.finish));
    el(xml, "Duration", &format_duration_iso(task.duration));
    mspdi_progress::write_stop_resume(xml, project, task);
    el(xml, "Manual", if task.manual { "1" } else { "0" });
    el(xml, "Milestone", if task.milestone { "1" } else { "0" });
    el(xml, "Summary", if task.summary { "1" } else { "0" });
    el(xml, "Critical", if task.critical { "1" } else { "0" });
    el(
        xml,
        "EffortDriven",
        if task.effort_driven { "1" } else { "0" },
    );
    if task.recurrence.is_some() {
        el(xml, "Recurring", "1");
    }
    el(xml, "Priority", &task.priority.to_string());
    el(xml, "Notes", &escape(&task.notes));
    el(
        xml,
        "ConstraintType",
        &constraint_to_xml(task.constraint_type).to_string(),
    );
    if let Some(date) = task.constraint_date {
        el(xml, "ConstraintDate", &format_iso(date));
    }
    mspdi_progress::write_deadline(xml, task);
    if let Some(calendar_id) = task.calendar_id {
        el(xml, "CalendarUID", &calendar_id.to_string());
    }
    el(
        xml,
        "PercentComplete",
        &format_number(task.percent_complete),
    );
    if let Some(start) = task.actual_start {
        el(xml, "ActualStart", &format_iso(start));
    }
    if let Some(finish) = task.actual_finish {
        el(xml, "ActualFinish", &format_iso(finish));
    }
    el(
        xml,
        "RemainingDuration",
        &format_duration_iso(task.remaining_duration),
    );
    mspdi_progress::write_task_progress(xml, task);
    mspdi_cost::write_task_costs(xml, task);
    el(xml, "FixedCost", &format_number(task.fixed_cost));
    el(
        xml,
        "FixedCostAccrual",
        match task.fixed_cost_accrual {
            Accrual::Start => "1",
            Accrual::Prorated => "2",
            Accrual::End => "3",
        },
    );
    el(
        xml,
        "LevelingDelay",
        &(task.leveling_delay * 10).to_string(),
    );
    for dep in project
        .dependencies
        .iter()
        .filter(|dep| dep.successor_uid == task.uid)
    {
        xml.push_str("<PredecessorLink>\n");
        el(xml, "PredecessorUID", &dep.predecessor_uid.to_string());
        el(xml, "Type", &dep_type_xml(dep.kind).to_string());
        el(xml, "LinkLag", &(dep.lag * 10).to_string());
        el(xml, "LagFormat", "3");
        xml.push_str("</PredecessorLink>\n");
    }
    mspdi_fields::write_values(xml, project, task);
    mspdi_range::write_values(xml, task);
    for baseline in &task.baselines {
        xml.push_str("<Baseline>\n");
        el(xml, "Number", &baseline.number.to_string());
        el(xml, "Start", &format_iso(baseline.start));
        el(xml, "Finish", &format_iso(baseline.finish));
        el(xml, "Work", &format_duration_iso(baseline.work));
        el(xml, "Cost", &format_number(baseline.cost));
        xml.push_str("</Baseline>\n");
    }
    mspdi_baseline::write(xml, project, task);
    xml.push_str("</Task>\n");
}

fn write_resource(xml: &mut String, project: &Project, resource: &Resource) {
    xml.push_str("<Resource>\n");
    el(xml, "UID", &resource.uid.to_string());
    el(xml, "Name", &escape(&resource.name));
    el(xml, "Type", "1");
    // The group is the resource's role; without one it names the kind, as before roles,
    // which is how an import tells machines from people.
    let group = if resource.group.is_empty() {
        match resource.kind {
            ResourceKind::Person => "Người",
            ResourceKind::Machine => "Máy",
        }
    } else {
        resource.group.as_str()
    };
    el(xml, "Group", &escape(group));
    if !resource.email.is_empty() {
        el(xml, "EmailAddress", &escape(&resource.email));
    }
    el(xml, "MaxUnits", &format_number(resource.max_units));
    el(xml, "StandardRate", &format_number(resource.standard_rate));
    el(xml, "OvertimeRate", &format_number(resource.overtime_rate));
    el(xml, "CostPerUse", &format_number(resource.cost_per_use));
    mspdi_cost::write_resource_costs(xml, project, resource);
    if let Some(calendar_id) = resource.calendar_id {
        el(xml, "CalendarUID", &calendar_id.to_string());
    }
    mspdi_skills::write_value(xml, project, resource);
    mspdi_availability::write(xml, resource);
    xml.push_str("</Resource>\n");
}

fn write_assignment(xml: &mut String, project: &Project, assignment: &Assignment) {
    xml.push_str("<Assignment>\n");
    el(xml, "UID", &assignment.uid.to_string());
    el(xml, "TaskUID", &assignment.task_uid.to_string());
    el(xml, "ResourceUID", &assignment.resource_uid.to_string());
    let rate = assignment.units * crate::focus::focus_of_uid(project, assignment.resource_uid);
    el(xml, "Units", &format_number(rate));
    el(xml, "Work", &format_duration_iso(assignment.work));
    el(
        xml,
        "OvertimeWork",
        &format_duration_iso(assignment.overtime_work),
    );
    el(xml, "Cost", &format_number(assignment.cost));
    mspdi_cost::write_assignment_costs(xml, assignment);
    // The schema puts an assignment's baselines before its timephased data.
    for baseline in &assignment.baselines {
        xml.push_str("<Baseline>\n");
        el(xml, "Number", &baseline.number.to_string());
        el(xml, "Work", &format_duration_iso(baseline.work));
        el(xml, "Cost", &format_number(baseline.cost));
        xml.push_str("</Baseline>\n");
    }
    if let Some(task) = project.task(assignment.task_uid) {
        write_timephased(xml, project, task, assignment.uid, assignment.work);
        mspdi_progress::write_actual_timephased(
            xml,
            project,
            task,
            assignment.uid,
            assignment.actual_work,
        );
    }
    xml.push_str("</Assignment>\n");
}

fn el(xml: &mut String, name: &str, value: &str) {
    xml.push('<');
    xml.push_str(name);
    xml.push('>');
    xml.push_str(value);
    xml.push_str("</");
    xml.push_str(name);
    xml.push_str(">\n");
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(crate) fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn format_clock(minute: u16) -> String {
    format!("{:02}:{:02}:00", minute / 60, minute % 60)
}

fn parse_clock(text: &str) -> u16 {
    let mut parts = text.split(':');
    let hour: u16 = parts.next().unwrap_or("0").parse().unwrap_or(0);
    let minute: u16 = parts.next().unwrap_or("0").parse().unwrap_or(0);
    hour * 60 + minute
}

fn local_name(qname: &[u8]) -> String {
    let text = String::from_utf8_lossy(qname);
    match text.rfind('}') {
        Some(index) => text[index + 1..].to_string(),
        None => text.to_string(),
    }
}

fn index_to_day_type(index: usize) -> u8 {
    match index {
        0 => 2,
        1 => 3,
        2 => 4,
        3 => 5,
        4 => 6,
        5 => 7,
        _ => 1,
    }
}

fn day_type_index(day_type: u8) -> Option<usize> {
    match day_type {
        1 => Some(6),
        2 => Some(0),
        3 => Some(1),
        4 => Some(2),
        5 => Some(3),
        6 => Some(4),
        7 => Some(5),
        _ => None,
    }
}

fn constraint_to_xml(constraint: ConstraintType) -> i32 {
    match constraint {
        ConstraintType::Asap => 0,
        ConstraintType::Alap => 1,
        ConstraintType::Mso => 2,
        ConstraintType::Mfo => 3,
        ConstraintType::Snet => 4,
        ConstraintType::Snlt => 5,
        ConstraintType::Fnet => 6,
        ConstraintType::Fnlt => 7,
    }
}

fn constraint_from_xml(value: i32) -> Option<ConstraintType> {
    Some(match value {
        0 => ConstraintType::Asap,
        1 => ConstraintType::Alap,
        2 => ConstraintType::Mso,
        3 => ConstraintType::Mfo,
        4 => ConstraintType::Snet,
        5 => ConstraintType::Snlt,
        6 => ConstraintType::Fnet,
        7 => ConstraintType::Fnlt,
        _ => return None,
    })
}

fn task_type_xml(task_type: TaskType) -> i32 {
    match task_type {
        TaskType::FixedUnits => 0,
        TaskType::FixedDuration => 1,
        TaskType::FixedWork => 2,
    }
}

fn dep_type_xml(kind: DepKind) -> i32 {
    match kind {
        DepKind::Ff => 0,
        DepKind::Fs => 1,
        DepKind::Ss => 2,
        DepKind::Sf => 3,
    }
}

#[derive(Default)]
struct CalendarBuilder {
    id: u32,
    name: String,
    week: [Vec<TimeRange>; 7],
    exceptions: BTreeMap<String, Vec<TimeRange>>,
    work_weeks: Vec<WorkWeek>,
    weeks_without_dates: usize,
    /// `DayType` 0 entries with no usable `FromDate`.
    exceptions_without_dates: usize,
    base_uid: Option<u32>,
    /// Weekdays the file lists; a based calendar takes the others from its base.
    listed: [bool; 7],
}

impl CalendarBuilder {
    fn finish(self, notes: &mut Vec<ImportNote>) -> Calendar {
        let mut calendar = Calendar {
            id: if self.id == 0 { 1 } else { self.id },
            name: if self.name.is_empty() {
                "Chuẩn".into()
            } else {
                self.name
            },
            week: self.week,
            exceptions: self.exceptions,
            work_weeks: Vec::new(),
            base: None,
        };
        for _ in 0..self.weeks_without_dates {
            notes.push(ImportNote::WorkWeekWithoutDates { calendar: calendar.name.clone() });
        }
        for _ in 0..self.exceptions_without_dates {
            notes.push(ImportNote::ExceptionWithoutDates { calendar: calendar.name.clone() });
        }
        mspdi_workweeks::add_all(&mut calendar, self.work_weeks, notes);
        if let Some(base_id) = self.base_uid.filter(|id| *id != calendar.id) {
            // Only its own part for now; `calendar_base::refresh` merges the base in.
            let mut week: [Option<Vec<TimeRange>>; 7] = Default::default();
            for (index, day) in week.iter_mut().enumerate() {
                if self.listed[index] {
                    *day = Some(calendar.week[index].clone());
                }
            }
            calendar.base = Some(CalendarBase {
                id: base_id,
                week,
                exceptions: calendar.exceptions.clone(),
                work_weeks: calendar.work_weeks.clone(),
            });
        }
        calendar
    }
}

#[derive(Default)]
struct WeekdayBuilder {
    /// `None` when the element is missing or not a number.
    day_type: Option<u8>,
    working: bool,
    ranges: Vec<TimeRange>,
    /// The `TimePeriod` of a `DayType` 0 exception.
    from: Option<String>,
    to: Option<String>,
}

#[derive(Default)]
struct ExceptionBuilder {
    working: bool,
    from: Option<String>,
    to: Option<String>,
    ranges: Vec<TimeRange>,
}

#[derive(Clone, Default)]
struct LinkBuilder {
    predecessor: u32,
    kind: DepKind,
    lag: i64,
}

#[derive(Default)]
struct TaskBuilder {
    uid: u32,
    name: String,
    outline_level: u32,
    duration: i64,
    milestone: bool,
    manual: bool,
    notes: String,
    priority: i32,
    constraint: Option<ConstraintType>,
    constraint_date: Option<i64>,
    calendar_id: Option<u32>,
    task_type: TaskType,
    effort_driven: bool,
    percent: f64,
    percent_given: bool,
    actual_start: Option<i64>,
    actual_finish: Option<i64>,
    remaining_duration: Option<i64>,
    start: Option<i64>,
    finish: Option<i64>,
    fixed_cost: f64,
    accrual: Accrual,
    leveling_delay: i64,
    is_null: bool,
    progress: TaskProgress,
    links: Vec<LinkBuilder>,
    baselines: Vec<Baseline>,
}

impl TaskBuilder {
    fn finish(self) -> Task {
        let name = if self.name.is_empty() {
            "Công việc".to_string()
        } else {
            self.name.clone()
        };
        let mut task = Task::new(self.uid, name);
        task.outline_level = self.outline_level.max(1);
        task.duration = if self.milestone { 0 } else { self.duration };
        task.milestone = self.milestone || task.duration == 0;
        task.manual = self.manual;
        task.notes = self.notes;
        task.priority = if self.priority == 0 {
            500
        } else {
            self.priority
        };
        if let Some(constraint) = self.constraint {
            task.constraint_type = constraint;
        }
        task.constraint_date = self.constraint_date;
        task.calendar_id = self.calendar_id;
        task.task_type = self.task_type;
        task.effort_driven = self.effort_driven || self.task_type == TaskType::FixedWork;
        task.percent_complete = self.percent;
        // Progress without actual dates started (and finished) where it was scheduled.
        task.actual_start = self
            .actual_start
            .or(self.start.filter(|_| self.percent > 0.0));
        task.actual_finish = self
            .actual_finish
            .or(self.finish.filter(|_| self.percent >= 100.0));
        task.remaining_duration = self.remaining_duration.unwrap_or(task.duration);
        if let Some(start) = self.start {
            task.start = start;
        }
        if let Some(finish) = self.finish {
            task.finish = finish;
        }
        task.fixed_cost = self.fixed_cost;
        task.fixed_cost_accrual = self.accrual;
        task.leveling_delay = self.leveling_delay;
        task.deadline = self.progress.deadline;
        task.manual_actual_cost = self.progress.actual_cost;
        task.work = task.duration;
        task.baselines = self.baselines;
        let dependencies = self.links;
        // Dependencies are attached by the caller via a side channel: stash them in notes? No.
        // The builder is converted in read_mspdi before dependencies exist on the task.
        // Store links in the task by pushing after finish — handled below by returning links separately.
        let _ = dependencies;
        task
    }
}

/// An address means one person in a project: a file that gives the same one to two people
/// keeps it with the first, and an address that is not one is dropped.
fn drop_repeated_emails(project: &mut Project) {
    let mut seen = std::collections::HashSet::new();
    for resource in &mut project.resources {
        let lower = resource.email.to_lowercase();
        if !lower.is_empty() && (!crate::source::looks_like_email(&resource.email) || !seen.insert(lower)) {
            resource.email.clear();
        }
    }
}

#[derive(Default)]
struct ResourceBuilder {
    uid: u32,
    name: String,
    group: String,
    email: String,
    max_units: f64,
    standard_rate: f64,
    overtime_rate: f64,
    cost_per_use: f64,
    calendar_id: Option<u32>,
    periods: Vec<AvailabilityPeriod>,
}

impl ResourceBuilder {
    fn finish(self, notes: &mut Vec<ImportNote>) -> Resource {
        let kind = if self.group.contains("Máy") || self.group.eq_ignore_ascii_case("machine") {
            ResourceKind::Machine
        } else {
            ResourceKind::Person
        };
        let mut resource = Resource {
            uid: self.uid,
            name: if self.name.is_empty() {
                format!("Nguồn lực {}", self.uid)
            } else {
                self.name
            },
            kind,
            max_units: if self.max_units <= 0.0 {
                1.0
            } else {
                self.max_units
            },
            calendar_id: self.calendar_id,
            standard_rate: self.standard_rate,
            overtime_rate: self.overtime_rate,
            cost_per_use: self.cost_per_use,
            availability: Vec::new(),
            skills: Vec::new(),
            email: self.email,
            group: role_of(&self.group),
            skill_levels: BTreeMap::new(),
            focus: None,
        };
        mspdi_availability::apply(&mut resource, self.periods, notes);
        resource
    }
}

/// The role a file's `<Group>` holds: none when it only names a kind.
fn role_of(group: &str) -> String {
    let group = group.trim();
    let kind_only = ["người", "máy", "person", "machine"].contains(&group.to_lowercase().as_str());
    if kind_only {
        return String::new();
    }
    group.chars().take(crate::skill_levels::MAX_GROUP_CHARS).collect()
}

#[derive(Default)]
struct AssignmentBuilder {
    uid: u32,
    task_uid: u32,
    resource_uid: u32,
    units: f64,
    work: i64,
    overtime_work: i64,
    actual_cost: Option<f64>,
    periods: Vec<Period>,
    baselines: Vec<AssignmentBaseline>,
}

/// One timephased work record of an assignment.
#[derive(Clone, Default)]
struct Period {
    kind: u32,
    start: i64,
    finish: i64,
    minutes: i64,
}

impl AssignmentBuilder {
    fn finish(self) -> Assignment {
        Assignment {
            uid: self.uid,
            task_uid: self.task_uid,
            resource_uid: self.resource_uid,
            units: if self.units <= 0.0 { 1.0 } else { self.units },
            work: self.work,
            overtime_work: self.overtime_work,
            actual_work: 0,
            remaining_work: self.work,
            cost: 0.0,
            actual_cost: 0.0,
            manual_actual_cost: self.actual_cost,
            baselines: self.baselines,
        }
    }
}

// The task builder drops links. Re-read them by keeping a parallel list.
// `read_mspdi` pushes `builder.finish()` and loses links. Fix that in `Task` conversion
// by writing dependencies from a thread-local? Better: change the push site.
// This helper is used by a second pass stored on the builder via a public field consumed
// before finish. See `take_links` below.

impl TaskBuilder {
    fn into_task_and_links(self, next_dep: &mut u32) -> (Task, Vec<Dependency>) {
        let links = self.links.clone();
        let uid = self.uid;
        let task = self.finish();
        let mut dependencies = Vec::new();
        for link in links {
            if link.predecessor == 0 {
                continue;
            }
            let dep_uid = *next_dep;
            *next_dep += 1;
            dependencies.push(Dependency {
                uid: dep_uid,
                predecessor_uid: link.predecessor,
                successor_uid: uid,
                kind: link.kind,
                lag: link.lag,
            });
        }
        (task, dependencies)
    }
}

/// A file's actual cost is an entry only where it differs from what the engine computes:
/// Project writes every actual cost, entered or not, and freezing the computed ones would
/// stop them following progress. Assignments are compared with their computed share, and
/// tasks with the total their assignment entries give. Returns whether entries remain,
/// so the caller schedules again with them.
fn keep_real_entries(project: &mut Project) -> bool {
    let tasks: Vec<Option<f64>> = project.tasks.iter_mut().map(|task| task.manual_actual_cost.take()).collect();
    let assignments: Vec<Option<f64>> = project
        .assignments
        .iter_mut()
        .map(|assignment| assignment.manual_actual_cost.take())
        .collect();
    if tasks.iter().chain(&assignments).all(Option::is_none) {
        return false;
    }
    let differs = |entry: Option<f64>, computed: f64| entry.filter(|cost| (cost - computed).abs() > 0.005);
    // Costs as computed with no entries at all.
    crate::work::publish_costs_and_usage(project);
    let mut any = false;
    for (assignment, entry) in project.assignments.iter_mut().zip(assignments) {
        assignment.manual_actual_cost = differs(entry, assignment.actual_cost);
        any |= assignment.manual_actual_cost.is_some();
    }
    crate::work::publish_costs_and_usage(project);
    for (task, entry) in project.tasks.iter_mut().zip(tasks) {
        task.manual_actual_cost = differs(entry, task.actual_cost);
        any |= task.manual_actual_cost.is_some();
    }
    any
}
