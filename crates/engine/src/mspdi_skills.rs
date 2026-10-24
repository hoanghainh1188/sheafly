//! Skills in MSPDI, as a resource text field ("Kỹ năng" or "Skills") whose value lists a
//! resource's skills separated by commas.
//!
//! Microsoft Project has no skills; its generic resources stand in for a person to be
//! named later, not a label, so they are not used. Export declares resource Text1
//! (`FieldID` 205520904, `pjCustomResourceText1` of `PjCustomField`) with the alias "Kỹ
//! năng" when a resource has a skill. Import reads any resource text field whose alias is
//! "Kỹ năng" or "Skills" (letter case aside), splits it at commas, and makes a skill of
//! each name; other resource fields are left out as before. A skill with a level is written
//! `Skill:Level` (`skill_levels.rs`); import adds the levels a file names to the scale in
//! the order met, at the top, since MSPDI has no scale to give their order. Rules are not
//! written: they stay in `.sheafly` files.

use std::collections::BTreeMap;

use crate::model::{Project, Resource};
use crate::mspdi::escape_text;
use crate::skill_levels::{self, LEVEL_SEPARATOR};
use crate::skills::{self, SkillDef, MAX_SKILLS, MAX_SKILL_NAME_CHARS};

/// `FieldID` of resource Text1, where skills are written.
const SKILLS_FIELD_ID: u32 = 205_520_904;
/// `FieldID` of resource Text1–Text30.
const RESOURCE_TEXT_IDS: [u32; 30] = [
    205520904, 205520905, 205520926, 205520927, 205520928, 205520993, 205520994, 205520995,
    205520996, 205520997, 205521121, 205521122, 205521123, 205521124, 205521125, 205521126,
    205521127, 205521128, 205521129, 205521130, 205521131, 205521132, 205521133, 205521134,
    205521135, 205521136, 205521137, 205521138, 205521139, 205521140,
];
/// The alias written, and the ones read.
const ALIAS: &str = "Kỹ năng";
const ALIASES: [&str; 2] = ["kỹ năng", "skills"];

/// Whether any resource has a skill: only then is the field declared and written.
pub(crate) fn any(project: &Project) -> bool {
    project
        .resources
        .iter()
        .any(|resource| !resource.skills.is_empty())
}

/// The field's declaration, inside `<ExtendedAttributes>`.
pub(crate) fn write_definition(xml: &mut String, project: &Project) {
    if !any(project) {
        return;
    }
    xml.push_str("<ExtendedAttribute>\n");
    el(xml, "FieldID", &SKILLS_FIELD_ID.to_string());
    el(xml, "FieldName", "Text1");
    el(xml, "CFType", "7");
    el(xml, "ElemType", "21");
    el(xml, "Alias", ALIAS);
    xml.push_str("</ExtendedAttribute>\n");
}

/// The resource's skills, names in the project's order of skills.
pub(crate) fn write_value(xml: &mut String, project: &Project, resource: &Resource) {
    if resource.skills.is_empty() {
        return;
    }
    let names: Vec<String> = project
        .skills
        .iter()
        .filter(|skill| resource.skills.contains(&skill.id))
        .map(|skill| {
            let level = resource
                .skill_levels
                .get(&skill.id)
                .and_then(|level| project.skill_levels.iter().find(|known| known.id == *level));
            match level {
                Some(level) => format!("{}{LEVEL_SEPARATOR}{}", skill.name, level.name),
                None => skill.name.clone(),
            }
        })
        .collect();
    xml.push_str("<ExtendedAttribute>\n");
    el(xml, "FieldID", &SKILLS_FIELD_ID.to_string());
    el(xml, "Value", &escape_text(&names.join(", ")));
    xml.push_str("</ExtendedAttribute>\n");
}

/// Collects the skills field's `FieldID` and resources' values while the file is read.
#[derive(Default)]
pub(crate) struct SkillReader {
    /// `FieldID` and alias of the declaration being read.
    declaring: Option<(u32, String)>,
    /// Resource text fields declared as skills.
    fields: Vec<u32>,
    /// `FieldID` and value of a resource's `<ExtendedAttribute>` being read.
    open_value: Option<(u32, String)>,
    resource_values: Vec<(u32, String)>,
    /// Values by resource uid, in file order.
    values: Vec<(u32, u32, String)>,
}

impl SkillReader {
    pub fn start(&mut self, name: &str, parent: Option<&str>) {
        match (parent, name) {
            (Some("ExtendedAttributes"), "ExtendedAttribute") => {
                self.declaring = Some((0, String::new()))
            }
            (Some("Resource"), "ExtendedAttribute") => self.open_value = Some((0, String::new())),
            (Some("Resources"), "Resource") => self.resource_values.clear(),
            _ => {}
        }
    }

    pub fn text(&mut self, name: &str, parent: Option<&str>, text: &str) {
        if parent != Some("ExtendedAttribute") {
            return;
        }
        let text = text.trim();
        if let Some((field_id, alias)) = self.declaring.as_mut() {
            match name {
                "FieldID" => *field_id = text.parse().unwrap_or(0),
                "Alias" => *alias = text.into(),
                _ => {}
            }
        } else if let Some((field_id, value)) = self.open_value.as_mut() {
            match name {
                "FieldID" => *field_id = text.parse().unwrap_or(0),
                "Value" => *value = text.into(),
                _ => {}
            }
        }
    }

    pub fn end(&mut self, name: &str, parent: Option<&str>) {
        match (parent, name) {
            (Some("ExtendedAttributes"), "ExtendedAttribute") => {
                if let Some((field_id, alias)) = self.declaring.take() {
                    let skills = ALIASES.contains(&alias.to_lowercase().as_str());
                    if skills && RESOURCE_TEXT_IDS.contains(&field_id) {
                        self.fields.push(field_id);
                    }
                }
            }
            (Some("Resource"), "ExtendedAttribute") => {
                if let Some(value) = self.open_value.take() {
                    self.resource_values.push(value);
                }
            }
            _ => {}
        }
    }

    /// The resource with `uid` was read: its values belong to it.
    pub fn end_resource(&mut self, uid: u32) {
        for (field_id, value) in self.resource_values.drain(..) {
            self.values.push((uid, field_id, value));
        }
    }

    /// Make a skill of every name, in the order met, and give resources theirs. Names that
    /// differ only in letter case are one skill; past the limit, new names are left out.
    pub fn finish(self, project: &mut Project) {
        let mut ids: BTreeMap<String, u32> = project
            .skills
            .iter()
            .map(|skill| (skill.name.to_lowercase(), skill.id))
            .collect();
        for (uid, field_id, value) in self.values {
            if !self.fields.contains(&field_id) {
                continue;
            }
            let mut skills: Vec<u32> = Vec::new();
            let mut levels: Vec<(u32, u32)> = Vec::new();
            for text in value.split(',') {
                let (name, level) = skill_levels::split_level(text);
                let name: String = name.chars().take(MAX_SKILL_NAME_CHARS).collect();
                let name = name.trim();
                if name.is_empty() {
                    continue;
                }
                let id = match ids.get(&name.to_lowercase()) {
                    Some(&id) => id,
                    None if project.skills.len() < MAX_SKILLS => {
                        let id = new_skill(project, name);
                        ids.insert(name.to_lowercase(), id);
                        id
                    }
                    None => continue,
                };
                skills.push(id);
                // A level the scale cannot take (a bad name, a full scale) is left out.
                if let Some(level) =
                    level.and_then(|level| skill_levels::level_id(project, level).ok())
                {
                    levels.push((id, level));
                }
            }
            if let Some(resource) = project
                .resources
                .iter_mut()
                .find(|resource| resource.uid == uid)
            {
                resource.skills.extend(skills);
                resource.skills.sort_unstable();
                resource.skills.dedup();
                resource.skill_levels.extend(levels);
            }
        }
    }
}

fn new_skill(project: &mut Project, name: &str) -> u32 {
    let id = skills::alloc_skill_id(project);
    project.skills.push(SkillDef {
        id,
        name: name.to_string(),
    });
    id
}

fn el(xml: &mut String, name: &str, value: &str) {
    xml.push_str(&format!("<{name}>{value}</{name}>\n"));
}
