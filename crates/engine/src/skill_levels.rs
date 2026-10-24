//! Skill levels and resource groups.
//!
//! The project names an ordered scale of levels (`Project.skill_levels`, lowest first, for
//! example Junior, Mid, Senior); an empty scale means levels are not used. A resource holds
//! a level per skill it has (`Resource.skill_levels`, skill id → level id) or none, which
//! ranks below every level. A rule aimed at a skill can ask for a minimum level
//! (`RuleTarget::Skill::min_level`): only people with the skill at that level or above
//! qualify. Level ids are never reused, like skill ids, so a rule or a person never comes
//! to mean another level after the scale changes.
//!
//! `SetSkillLevels` replaces the scale: an entry with an id keeps that level (renamed or
//! moved), one without makes a new level. A level left out is removed; the people on it
//! and the rules asking for it move to the nearest kept level below it in the old order,
//! or to no level when none is kept below.
//!
//! A resource's group is its role or team, a label (`Resource.group`); scheduling never
//! reads it.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::error::{EngineError, Invalid};
use crate::model::{Project, Resource};
use crate::skills::RuleTarget;

/// Levels a scale can have.
pub const MAX_SKILL_LEVELS: usize = 20;
/// Longest level name.
pub const MAX_SKILL_LEVEL_NAME_CHARS: usize = 30;
/// Longest resource group.
pub const MAX_GROUP_CHARS: usize = 100;
/// What separates a skill from its level in text (MSPDI, table import): `Backend:Senior`.
pub const LEVEL_SEPARATOR: char = ':';

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillLevel {
    pub id: u32,
    pub name: String,
}

/// The level a resource has in one skill, as `SetResourceSkills` sends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillLevelOf {
    pub skill: u32,
    pub level: u32,
}

/// One entry of `SetSkillLevels`: an existing level by id, or a new one without.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillLevelInput {
    #[serde(default)]
    pub id: Option<u32>,
    pub name: String,
}

/// Replace the scale, lowest first.
pub(crate) fn set_levels(
    project: &mut Project,
    levels: &[SkillLevelInput],
) -> Result<(), EngineError> {
    if levels.len() > MAX_SKILL_LEVELS {
        return Err(Invalid::TooManySkillLevels.into());
    }
    let mut names: Vec<String> = Vec::with_capacity(levels.len());
    let mut kept: Vec<u32> = Vec::new();
    for level in levels {
        let name = checked_name(&level.name)?;
        if names
            .iter()
            .any(|other| other.to_lowercase() == name.to_lowercase())
        {
            return Err(Invalid::SkillLevelNameTaken.into());
        }
        names.push(name);
        if let Some(id) = level.id {
            if !project.skill_levels.iter().any(|old| old.id == id) || kept.contains(&id) {
                return Err(Invalid::NoSuchSkillLevel.into());
            }
            kept.push(id);
        }
    }
    let replacement = replacements(project, &kept);
    let mut scale = Vec::with_capacity(levels.len());
    for (level, name) in levels.iter().zip(names) {
        let id = match level.id {
            Some(id) => id,
            None => alloc_level_id(project),
        };
        scale.push(SkillLevel { id, name });
    }
    project.skill_levels = scale;
    for resource in &mut project.resources {
        let levels = std::mem::take(&mut resource.skill_levels);
        resource.skill_levels = levels
            .into_iter()
            .filter_map(|(skill, level)| match replacement.get(&level) {
                Some(Some(to)) => Some((skill, *to)),
                Some(None) => None,
                None => Some((skill, level)),
            })
            .collect();
    }
    for rule in &mut project.assignment_rules {
        if let RuleTarget::Skill { min_level, .. } = &mut rule.target {
            if let Some(to) = (*min_level).and_then(|level| replacement.get(&level)) {
                *min_level = *to;
            }
        }
    }
    Ok(())
}

/// For each level of the old scale not in `kept`: the nearest kept level below it, if any.
fn replacements(project: &Project, kept: &[u32]) -> HashMap<u32, Option<u32>> {
    let mut below: Option<u32> = None;
    let mut map = HashMap::new();
    for level in &project.skill_levels {
        if kept.contains(&level.id) {
            below = Some(level.id);
        } else {
            map.insert(level.id, below);
        }
    }
    map
}

/// A level id no level has had: past `next_skill_level_id` and every id in use.
pub(crate) fn alloc_level_id(project: &mut Project) -> u32 {
    let largest = project
        .skill_levels
        .iter()
        .map(|level| level.id)
        .max()
        .unwrap_or(0);
    let id = project.next_skill_level_id.max(largest + 1);
    project.next_skill_level_id = id + 1;
    id
}

/// A trimmed level name. A comma or a colon would break the `Skill:Level` lists of MSPDI
/// and table import, so they are refused.
pub(crate) fn checked_name(name: &str) -> Result<String, EngineError> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > MAX_SKILL_LEVEL_NAME_CHARS
        || name.contains(',')
        || name.contains(LEVEL_SEPARATOR)
    {
        return Err(Invalid::SkillLevelName.into());
    }
    Ok(name.to_string())
}

/// The levels of a resource's skills, checked against its skills and the scale; a later
/// entry for the same skill wins.
pub(crate) fn checked_resource_levels(
    project: &Project,
    skills: &[u32],
    levels: &[SkillLevelOf],
) -> Result<BTreeMap<u32, u32>, EngineError> {
    let mut checked = BTreeMap::new();
    for entry in levels {
        if !skills.contains(&entry.skill) {
            return Err(Invalid::LevelWithoutSkill.into());
        }
        if !project
            .skill_levels
            .iter()
            .any(|known| known.id == entry.level)
        {
            return Err(Invalid::NoSuchSkillLevel.into());
        }
        checked.insert(entry.skill, entry.level);
    }
    Ok(checked)
}

/// The position of `level` on the scale, lowest 0.
pub fn rank(project: &Project, level: u32) -> Option<usize> {
    project
        .skill_levels
        .iter()
        .position(|known| known.id == level)
}

/// Whether `resource` has skill `id` at `min_level` or above (any level when `None`).
pub fn qualifies(project: &Project, resource: &Resource, id: u32, min_level: Option<u32>) -> bool {
    if !resource.skills.contains(&id) {
        return false;
    }
    let Some(min_rank) = min_level.and_then(|level| rank(project, level)) else {
        return true;
    };
    resource
        .skill_levels
        .get(&id)
        .and_then(|level| rank(project, *level))
        .is_some_and(|own| own >= min_rank)
}

/// Split `Skill:Level` at its last colon; text without one is a skill alone.
pub fn split_level(text: &str) -> (&str, Option<&str>) {
    match text.rsplit_once(LEVEL_SEPARATOR) {
        Some((skill, level)) if !level.trim().is_empty() => (skill.trim(), Some(level.trim())),
        Some((skill, _)) => (skill.trim(), None),
        None => (text.trim(), None),
    }
}

/// The id of the level named `name` (letter case aside), adding it at the top of the scale
/// when the project lacks it. Used by imports, which have no scale of their own.
pub(crate) fn level_id(project: &mut Project, name: &str) -> Result<u32, EngineError> {
    let name = checked_name(name)?;
    let lower = name.to_lowercase();
    if let Some(level) = project
        .skill_levels
        .iter()
        .find(|level| level.name.to_lowercase() == lower)
    {
        return Ok(level.id);
    }
    if project.skill_levels.len() >= MAX_SKILL_LEVELS {
        return Err(Invalid::TooManySkillLevels.into());
    }
    let id = alloc_level_id(project);
    project.skill_levels.push(SkillLevel { id, name });
    Ok(id)
}

/// Set (or, empty, clear) a resource's group.
pub(crate) fn set_group(project: &mut Project, uid: u32, group: &str) -> Result<(), EngineError> {
    let group = checked_group(group)?;
    let resource = project
        .resources
        .iter_mut()
        .find(|resource| resource.uid == uid)
        .ok_or(EngineError::ResourceNotFound)?;
    resource.group = group;
    Ok(())
}

pub(crate) fn checked_group(group: &str) -> Result<String, EngineError> {
    let group = group.trim();
    if group.chars().count() > MAX_GROUP_CHARS {
        return Err(Invalid::ResourceGroup.into());
    }
    Ok(group.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_level_takes_the_last_colon() {
        assert_eq!(split_level("Backend:Senior"), ("Backend", Some("Senior")));
        assert_eq!(split_level(" QA "), ("QA", None));
        assert_eq!(split_level("A:B:Mid"), ("A:B", Some("Mid")));
        assert_eq!(split_level("Design:"), ("Design", None));
    }
}
