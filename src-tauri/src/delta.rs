//! Snapshots sent after an edit, undo or redo carry only what changed since the snapshot
//! the frontend holds. A project of 5,000 tasks is about 7.5 MB of JSON, and sending it
//! whole after every edit cost most of the time an edit takes in the webview.
//!
//! The project's large lists (tasks, resources, assignments, links, usage) become a
//! `ListPatch`: the items that differ, the keys gone, and the new key order when it
//! changed. Everything else in the project is small and goes whole, so a field added to
//! `Project` later reaches the frontend without a change here. Every answer carries `rev`;
//! a patch also names the `base` it applies to, and the frontend asks for the whole
//! snapshot (`project_state`) when it does not hold that one.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::Mutex;

use serde::Serialize;
use serde_json::{json, Map, Value};
use sheafly_engine::{Project, Snapshot, UsageRow};

/// The last snapshot sent and its number.
#[derive(Default)]
pub struct Delta {
    sent: Mutex<Option<(u64, Project)>>,
}

impl Delta {
    /// The whole snapshot, remembered as the base of the next patch.
    pub fn full(&self, snapshot: Snapshot) -> Value {
        let mut sent = self.sent.lock().unwrap_or_else(|err| err.into_inner());
        let rev = sent.as_ref().map_or(1, |(rev, _)| rev + 1);
        let mut value = to_object(&snapshot);
        value.insert("rev".into(), json!(rev));
        *sent = Some((rev, snapshot.project));
        Value::Object(value)
    }

    /// What changed since the snapshot sent last, or the whole snapshot when none was.
    pub fn patch(&self, snapshot: Snapshot) -> Value {
        let mut sent = self.sent.lock().unwrap_or_else(|err| err.into_inner());
        let Some((base, previous)) = sent.as_ref() else {
            drop(sent);
            return self.full(snapshot);
        };
        let (base, rev) = (*base, base + 1);
        let project = project_patch(previous, &snapshot.project);
        let value = json!({
            "patch": true,
            "base": base,
            "rev": rev,
            "project": project,
            "path": snapshot.path,
            "dirty": snapshot.dirty,
            "can_undo": snapshot.can_undo,
            "can_redo": snapshot.can_redo,
            "history_cursor": snapshot.history_cursor,
        });
        *sent = Some((rev, snapshot.project));
        value
    }
}

fn to_object<T: Serialize>(value: &T) -> Map<String, Value> {
    match serde_json::to_value(value) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

/// The project with each large list replaced by its `ListPatch` against `previous`.
pub fn project_patch(previous: &Project, next: &Project) -> Value {
    // The small fields, serialized without copying the large lists; those come next.
    let mut map = to_object(&small_fields(next));
    map.insert("tasks".into(), list_patch(&previous.tasks, &next.tasks, |task| task.uid));
    map.insert("resources".into(), list_patch(&previous.resources, &next.resources, |item| item.uid));
    map.insert("assignments".into(), list_patch(&previous.assignments, &next.assignments, |item| item.uid));
    map.insert("dependencies".into(), list_patch(&previous.dependencies, &next.dependencies, |item| item.uid));
    map.insert("usage".into(), list_patch(&previous.usage, &next.usage, usage_key));
    Value::Object(map)
}

/// A copy of `project` without its large lists, which `project_patch` sends apart.
fn small_fields(project: &Project) -> Project {
    Project {
        name: project.name.clone(),
        start: project.start,
        status_date: project.status_date,
        calendar_id: project.calendar_id,
        calendars: project.calendars.clone(),
        tasks: Vec::new(),
        dependencies: Vec::new(),
        resources: Vec::new(),
        assignments: Vec::new(),
        next_task_uid: project.next_task_uid,
        next_dependency_uid: project.next_dependency_uid,
        next_resource_uid: project.next_resource_uid,
        next_assignment_uid: project.next_assignment_uid,
        next_calendar_uid: project.next_calendar_uid,
        overallocations: project.overallocations.clone(),
        usage: Vec::new(),
        editable_actual_costs: project.editable_actual_costs,
        fields: project.fields.clone(),
        next_field_id: project.next_field_id,
        leveling_order: project.leveling_order.clone(),
        skills: project.skills.clone(),
        next_skill_id: project.next_skill_id,
        skill_levels: project.skill_levels.clone(),
        next_skill_level_id: project.next_skill_level_id,
        assignment_rules: project.assignment_rules.clone(),
        forecast: project.forecast.clone(),
        source: project.source.clone(),
    }
}

/// A usage row is one task, or one assignment of it.
pub fn usage_key(row: &UsageRow) -> String {
    match row.assignment_uid {
        Some(assignment) => format!("{}:{assignment}", row.task_uid),
        None => format!("{}:", row.task_uid),
    }
}

/// `{ changed, removed, order }`: the items of `next` that are new or differ, the keys of
/// `previous` no longer there, and every key of `next` in order when the order or the set
/// of keys changed (else null).
pub fn list_patch<T, K>(previous: &[T], next: &[T], key: impl Fn(&T) -> K) -> Value
where
    T: PartialEq + Serialize,
    K: Eq + Hash + Serialize,
{
    let before: HashMap<K, &T> = previous.iter().map(|item| (key(item), item)).collect();
    let changed: Vec<&T> = next
        .iter()
        .filter(|item| before.get(&key(item)).is_none_or(|old| *old != *item))
        .collect();
    let kept: HashSet<K> = next.iter().map(&key).collect();
    let removed: Vec<K> = previous.iter().map(&key).filter(|item| !kept.contains(item)).collect();
    let same_order = previous.len() == next.len() && previous.iter().zip(next).all(|(old, new)| key(old) == key(new));
    let order: Option<Vec<K>> = (!same_order).then(|| next.iter().map(&key).collect());
    json!({ "changed": changed, "removed": removed, "order": order })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sheafly_engine::{Command, Document};

    fn doc() -> Document {
        let mut doc = Document::new_default();
        for name in ["A", "B", "C"] {
            doc.apply(Command::AddTask { name: name.into(), after_uid: None }).unwrap();
        }
        doc
    }

    #[test]
    fn a_patch_holds_only_the_changed_tasks() {
        let mut doc = doc();
        let delta = Delta::default();
        let full = delta.full(doc.snapshot());
        assert_eq!(full["rev"], 1);
        let uid = doc.project.tasks[1].uid;
        let patch = delta.patch(doc.apply(Command::RenameTask { uid, name: "B2".into() }).unwrap());
        assert_eq!(patch["patch"], true);
        assert_eq!((patch["base"].clone(), patch["rev"].clone()), (json!(1), json!(2)));
        let tasks = &patch["project"]["tasks"];
        let names: Vec<&str> = tasks["changed"].as_array().unwrap().iter().map(|task| task["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"B2"));
        assert!(names.len() < doc.project.tasks.len() + 1);
        assert_eq!(tasks["order"], Value::Null);
        assert_eq!(patch["project"]["name"], json!(doc.project.name));
        assert_eq!(patch["can_undo"], true);
    }

    #[test]
    fn a_removed_task_is_listed_with_the_new_order() {
        let mut doc = doc();
        let delta = Delta::default();
        delta.full(doc.snapshot());
        let uid = doc.project.tasks[1].uid;
        let patch = delta.patch(doc.apply(Command::DeleteTask { uid }).unwrap());
        let tasks = &patch["project"]["tasks"];
        assert_eq!(tasks["removed"], json!([uid]));
        let order: Vec<u64> = doc.project.tasks.iter().map(|task| u64::from(task.uid)).collect();
        assert_eq!(tasks["order"], json!(order));
    }

    #[test]
    fn without_a_base_the_answer_is_whole() {
        let delta = Delta::default();
        let value = delta.patch(doc().snapshot());
        assert!(value.get("patch").is_none());
        assert!(value["project"]["tasks"].is_array());
    }

    #[test]
    fn the_small_fields_are_every_project_field() {
        // A field added to `Project` must reach the frontend in a patch too.
        let project = doc().project;
        let whole = to_object(&project);
        let patched = project_patch(&project, &project);
        let mut keys: Vec<&String> = patched.as_object().unwrap().keys().collect();
        let mut expected: Vec<&String> = whole.keys().collect();
        keys.sort();
        expected.sort();
        assert_eq!(keys, expected);
        assert_eq!(to_object(&small_fields(&project))["fields"], whole["fields"]);
    }
}

#[cfg(test)]
mod size {
    use super::*;
    use sheafly_engine::{Command, Document};

    #[test]
    #[ignore = "measures payload sizes; run with --release --ignored --nocapture"]
    fn patch_size_on_a_large_project() {
        let mut doc = Document::new_default();
        let mut tasks = doc.project.tasks.clone();
        let template = tasks[0].clone();
        for uid in 2..=5000u32 {
            let mut task = template.clone();
            task.uid = uid;
            task.id = uid;
            task.name = format!("Task {uid}");
            tasks.push(task);
        }
        doc.project.tasks = tasks;
        doc.base = doc.project.clone();
        // Schedule once, so the measured edit changes only what it touches.
        let uid = doc.project.tasks[10].uid;
        doc.apply(Command::RenameTask { uid, name: "x".into() }).unwrap();
        let delta = Delta::default();
        let started = std::time::Instant::now();
        let full = serde_json::to_string(&delta.full(doc.snapshot())).unwrap();
        let full_time = started.elapsed();
        let uid = doc.project.tasks[2500].uid;
        let snapshot = doc.apply(Command::RenameTask { uid, name: "Đổi".into() }).unwrap();
        let started = std::time::Instant::now();
        let patch = serde_json::to_string(&delta.patch(snapshot)).unwrap();
        println!("full {} bytes in {:?}; patch {} bytes in {:?}", full.len(), full_time, patch.len(), started.elapsed());
    }
}
