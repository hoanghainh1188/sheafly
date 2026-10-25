//! Assignment rules in the project summary (`summary.rs`), as text a model reads:
//! `Mảng = Thanh toán | Báo cáo` → `skill Backend ≥ Senior`. With `mask` on, a field is
//! `Field 3`, a choice `Option 2` (its place in the list) and a text value `Value 1` (in the
//! order the rules first use it), so equal values still look equal.

use std::collections::HashMap;

use crate::fields::{FieldKind, FieldValue};
use crate::model::Project;
use crate::skills::{Condition, RuleTarget};
use crate::summary::{resource_ref, RuleLine};

pub(crate) fn rule_lines(project: &Project, mask: bool) -> Vec<RuleLine> {
    let mut masked = Masked::default();
    project
        .assignment_rules
        .iter()
        .map(|rule| RuleLine {
            when: rule
                .conditions
                .iter()
                .map(|condition| condition_text(project, condition, mask, &mut masked))
                .collect(),
            to: target_text(project, rule.target),
            units: rule.units,
        })
        .collect()
}

/// Text values already given a masked number, by field.
#[derive(Default)]
struct Masked {
    values: HashMap<(u32, String), usize>,
    next: HashMap<u32, usize>,
}

impl Masked {
    fn value(&mut self, field: u32, text: &str) -> String {
        let key = (field, text.to_string());
        if let Some(number) = self.values.get(&key) {
            return format!("Value {number}");
        }
        let next = self.next.entry(field).or_insert(0);
        *next += 1;
        self.values.insert(key, *next);
        format!("Value {next}")
    }
}

fn condition_text(
    project: &Project,
    condition: &Condition,
    mask: bool,
    masked: &mut Masked,
) -> String {
    let field = project
        .fields
        .iter()
        .find(|field| field.id == condition.field);
    let name = match (mask, field) {
        (false, Some(field)) => field.name.clone(),
        _ => format!("Field {}", condition.field),
    };
    let values: Vec<String> = condition
        .values
        .iter()
        .map(|value| match value {
            FieldValue::Number(number) => number.to_string(),
            FieldValue::Text(text) if !mask => text.clone(),
            FieldValue::Text(text) => {
                let place = field.and_then(|field| match &field.kind {
                    FieldKind::Choice { options } => {
                        options.iter().position(|option| option == text)
                    }
                    FieldKind::Text | FieldKind::Number => None,
                });
                match place {
                    Some(place) => format!("Option {}", place + 1),
                    None => masked.value(condition.field, text),
                }
            }
        })
        .collect();
    format!("{name} = {}", values.join(" | "))
}

fn target_text(project: &Project, target: RuleTarget) -> String {
    match target {
        RuleTarget::Resource { uid } => resource_ref(uid),
        RuleTarget::Skill { id, min_level } => {
            let skill = project
                .skills
                .iter()
                .find(|skill| skill.id == id)
                .map_or_else(|| format!("{id}"), |skill| skill.name.clone());
            let level = min_level
                .and_then(|level| project.skill_levels.iter().find(|known| known.id == level));
            match level {
                Some(level) => format!("skill {skill} ≥ {}", level.name),
                None => format!("skill {skill}"),
            }
        }
    }
}
