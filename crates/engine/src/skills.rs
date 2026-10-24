//! Skills and assignment rules.
//!
//! The project names skills and each resource holds some of them. Tasks have no skill
//! of their own: an assignment rule reads the task's custom field values, so a team that
//! wants a "skill needed" on its tasks makes a list field for it. The rules form an
//! ordered list; the first one whose conditions all hold for a task points at one person
//! or at everyone with a skill, with the units an assignment would get. Rules only feed
//! suggestions (`suggest.rs`): accepting them is an ordinary `AssignTasks` command, so
//! replay never reads a rule.
//!
//! Rules stay true to the data inside the commands that change it: deleting a field drops
//! every rule with a condition on it (dropping only the condition would widen the rule),
//! changing a field converts the condition values as it converts task values, and
//! deleting a person or a skill drops the rules that point at it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{EngineError, Invalid};
use crate::fields::{self, FieldKind, FieldValue};
use crate::model::{Project, Task};
use crate::skill_levels::{self, SkillLevelOf};

/// Skills a project can have.
pub const MAX_SKILLS: usize = 200;
/// Longest skill name.
pub const MAX_SKILL_NAME_CHARS: usize = 50;
/// Rules a project can have.
pub const MAX_RULES: usize = 200;
/// Conditions one rule can have.
pub const MAX_CONDITIONS: usize = 8;
/// Most units a rule can give.
pub const MAX_RULE_UNITS: f64 = 1000.0;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillDef {
    pub id: u32,
    pub name: String,
}

/// When a task matches a rule and whom it then goes to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssignmentRule {
    /// All must hold; none matches every task.
    pub conditions: Vec<Condition>,
    pub target: RuleTarget,
    /// Units of the assignment the rule suggests.
    #[serde(default = "one")]
    pub units: f64,
}

fn one() -> f64 {
    1.0
}

/// The task's value of `field` is one of `values`; a task with no value does not match.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub field: u32,
    pub values: Vec<FieldValue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleTarget {
    Resource {
        uid: u32,
    },
    /// Everyone with the skill, at `min_level` or above when set (`skill_levels.rs`);
    /// suggestions rank them.
    Skill {
        id: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_level: Option<u32>,
    },
}

/// One row of `AssignTasks`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskAssignment {
    pub task_uid: u32,
    pub resource_uid: u32,
    pub units: f64,
}

pub(crate) fn add_skill(project: &mut Project, name: &str) -> Result<u32, EngineError> {
    let name = checked_name(project, None, name)?;
    if project.skills.len() >= MAX_SKILLS {
        return Err(Invalid::TooManySkills.into());
    }
    let id = alloc_skill_id(project);
    project.skills.push(SkillDef { id, name });
    Ok(id)
}

/// A skill id no skill has had: past `next_skill_id` and every id in use.
pub(crate) fn alloc_skill_id(project: &mut Project) -> u32 {
    let largest = project
        .skills
        .iter()
        .map(|skill| skill.id)
        .max()
        .unwrap_or(0);
    let id = project.next_skill_id.max(largest + 1);
    project.next_skill_id = id + 1;
    id
}

pub(crate) fn rename_skill(project: &mut Project, id: u32, name: &str) -> Result<(), EngineError> {
    let position = position_of(project, id)?;
    project.skills[position].name = checked_name(project, Some(id), name)?;
    Ok(())
}

/// Remove a skill from the project and every resource, with the rules that point at it.
pub(crate) fn delete_skill(project: &mut Project, id: u32) -> Result<(), EngineError> {
    let position = position_of(project, id)?;
    project.skills.remove(position);
    for resource in &mut project.resources {
        resource.skills.retain(|skill| *skill != id);
        resource.skill_levels.remove(&id);
    }
    project
        .assignment_rules
        .retain(|rule| !matches!(rule.target, RuleTarget::Skill { id: skill, .. } if skill == id));
    Ok(())
}

/// Replace a resource's skills and their levels; skills kept sorted and without repeats.
pub(crate) fn set_resource_skills(
    project: &mut Project,
    uid: u32,
    skills: &[u32],
    levels: &[SkillLevelOf],
) -> Result<(), EngineError> {
    if skills.iter().any(|id| position_of(project, *id).is_err()) {
        return Err(Invalid::NoSuchSkill.into());
    }
    let levels = skill_levels::checked_resource_levels(project, skills, levels)?;
    let resource = project
        .resources
        .iter_mut()
        .find(|resource| resource.uid == uid)
        .ok_or(EngineError::ResourceNotFound)?;
    let mut skills = skills.to_vec();
    skills.sort_unstable();
    skills.dedup();
    resource.skills = skills;
    resource.skill_levels = levels;
    Ok(())
}

/// Replace the project's rules. Values are stored as the field holds them (trimmed text)
/// without repeats.
pub(crate) fn set_rules(
    project: &mut Project,
    rules: &[AssignmentRule],
) -> Result<(), EngineError> {
    if rules.len() > MAX_RULES {
        return Err(Invalid::TooManyRules.into());
    }
    let checked = rules
        .iter()
        .map(|rule| checked_rule(project, rule))
        .collect::<Result<Vec<_>, _>>()?;
    project.assignment_rules = checked;
    Ok(())
}

fn checked_rule(project: &Project, rule: &AssignmentRule) -> Result<AssignmentRule, EngineError> {
    if rule.conditions.len() > MAX_CONDITIONS {
        return Err(Invalid::TooManyConditions.into());
    }
    let mut conditions: Vec<Condition> = Vec::with_capacity(rule.conditions.len());
    for condition in &rule.conditions {
        if conditions
            .iter()
            .any(|earlier| earlier.field == condition.field)
        {
            return Err(Invalid::DuplicateCondition.into());
        }
        let kind = &project
            .fields
            .iter()
            .find(|field| field.id == condition.field)
            .ok_or(Invalid::NoSuchField)?
            .kind;
        if condition.values.is_empty() {
            return Err(Invalid::EmptyCondition.into());
        }
        let mut values: Vec<FieldValue> = Vec::with_capacity(condition.values.len());
        for value in &condition.values {
            let value = fields::checked_value(kind, value)?.ok_or(Invalid::FieldValue)?;
            if !values.contains(&value) {
                values.push(value);
            }
        }
        conditions.push(Condition {
            field: condition.field,
            values,
        });
    }
    match rule.target {
        RuleTarget::Resource { uid } => {
            if !project.resources.iter().any(|resource| resource.uid == uid) {
                return Err(EngineError::ResourceNotFound);
            }
        }
        RuleTarget::Skill { id, min_level } => {
            position_of(project, id)?;
            if min_level.is_some_and(|level| skill_levels::rank(project, level).is_none()) {
                return Err(Invalid::NoSuchSkillLevel.into());
            }
        }
    }
    let units = rule.units;
    if !units.is_finite() || units <= 0.0 || units > MAX_RULE_UNITS {
        return Err(Invalid::RuleUnits.into());
    }
    Ok(AssignmentRule {
        conditions,
        target: rule.target,
        units,
    })
}

/// The index of the first rule that matches `task`.
pub fn rule_for(project: &Project, task: &Task) -> Option<usize> {
    project
        .assignment_rules
        .iter()
        .position(|rule| matches(rule, task))
}

fn matches(rule: &AssignmentRule, task: &Task) -> bool {
    rule.conditions.iter().all(|condition| {
        task.values
            .get(&condition.field)
            .is_some_and(|value| condition.values.contains(value))
    })
}

/// A field was deleted: drop every rule with a condition on it.
pub(crate) fn drop_field(project: &mut Project, id: u32) {
    project.assignment_rules.retain(|rule| {
        rule.conditions
            .iter()
            .all(|condition| condition.field != id)
    });
}

/// A field changed kind or had choices renamed: its condition values follow, as task
/// values do; a condition left with no value takes its rule with it.
pub(crate) fn field_changed(
    project: &mut Project,
    id: u32,
    kind: &FieldKind,
    renames: &BTreeMap<String, String>,
) {
    project.assignment_rules.retain_mut(|rule| {
        rule.conditions
            .iter_mut()
            .filter(|condition| condition.field == id)
            .all(|condition| {
                let mut values: Vec<FieldValue> = Vec::with_capacity(condition.values.len());
                for value in condition.values.drain(..) {
                    let value = match value {
                        FieldValue::Text(text) => {
                            FieldValue::Text(renames.get(&text).cloned().unwrap_or(text))
                        }
                        number => number,
                    };
                    if let Some(value) = fields::converted(value, kind) {
                        if !values.contains(&value) {
                            values.push(value);
                        }
                    }
                }
                condition.values = values;
                !condition.values.is_empty()
            })
    });
}

/// A resource was deleted: drop the rules that point at it.
pub(crate) fn drop_resource(project: &mut Project, uid: u32) {
    project
        .assignment_rules
        .retain(|rule| rule.target != RuleTarget::Resource { uid });
}

fn position_of(project: &Project, id: u32) -> Result<usize, EngineError> {
    project
        .skills
        .iter()
        .position(|skill| skill.id == id)
        .ok_or_else(|| Invalid::NoSuchSkill.into())
}

/// A trimmed name no other skill has (letter case aside). A comma would split it in
/// MSPDI's comma-separated list and a colon would read as a level (`Skill:Level`), so
/// both are refused.
fn checked_name(project: &Project, own: Option<u32>, name: &str) -> Result<String, EngineError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_SKILL_NAME_CHARS || name.contains(',')
        || name.contains(skill_levels::LEVEL_SEPARATOR)
    {
        return Err(Invalid::SkillName.into());
    }
    let lower = name.to_lowercase();
    let taken = project
        .skills
        .iter()
        .any(|skill| Some(skill.id) != own && skill.name.to_lowercase() == lower);
    if taken {
        return Err(Invalid::SkillNameTaken.into());
    }
    Ok(name.to_string())
}
