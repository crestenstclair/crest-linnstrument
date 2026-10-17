use crate::Contact;
use crest_linnstrument::Geometry;
use serde::{Deserialize, Serialize};
use std::{io::Read, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactScenario {
    pub version: u32,
    pub model: u16,
    pub steps: Vec<ContactStep>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactStep {
    pub at_us: u64,
    pub action: ContactAction,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContactAction {
    Contact(Contact),
    Release { id: u32 },
    ReleaseAll {},
}

pub(crate) fn validate_contact(c: Contact, geometry: Geometry) -> Result<(), String> {
    if !c.x.is_finite()
        || !c.y.is_finite()
        || !c.z.is_finite()
        || c.x < 0.0
        || c.x > f64::from(geometry.columns() - 1)
        || c.y < 0.0
        || c.y > 7.0
        || c.z < 0.0
        || c.z > 1.0
        || c.id > i32::MAX as u32
    {
        return Err("invalid contact".into());
    }
    Ok(())
}
impl ContactScenario {
    pub fn example(model: u16) -> Result<Self, String> {
        let mut scenario: Self = serde_json::from_str(include_str!("../scenarios/expressive.json"))
            .map_err(|e| e.to_string())?;
        scenario.model = model;
        Ok(scenario)
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(1_000_001)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 1_000_000 {
            return Err("contact scenario exceeds 1 MB".into());
        }
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())
    }
    pub fn validate(&self, model: u16, geometry: Geometry) -> Result<(), String> {
        if self.version != 1
            || self.model != model
            || !matches!(
                (model, geometry),
                (128, Geometry::Pads128) | (200, Geometry::Pads200)
            )
            || self.steps.is_empty()
            || self.steps.len() > 4096
        {
            return Err("incompatible contact scenario".into());
        }
        let mut last = 0;
        let mut contacts = std::collections::BTreeSet::new();
        for step in &self.steps {
            if step.at_us < last || step.at_us > 60_000_000 {
                return Err("scenario times must be ordered within 60 seconds".into());
            }
            last = step.at_us;
            match step.action {
                ContactAction::Contact(c) => {
                    validate_contact(c, geometry)?;
                    contacts.insert(c.id);
                    if contacts.len() > 16 {
                        return Err("16 contact limit".into());
                    }
                }
                ContactAction::Release { id } => {
                    if !contacts.remove(&id) {
                        return Err("scenario releases unknown contact".into());
                    }
                }
                ContactAction::ReleaseAll {} => contacts.clear(),
            }
        }
        if !contacts.is_empty() {
            return Err("scenario must release every contact".into());
        }
        Ok(())
    }
}
pub(crate) struct Playback {
    pub started_us: u64,
    pub steps: std::collections::VecDeque<ContactStep>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scenarios_validate_geometry_times_and_complete_releases() {
        let mut example = ContactScenario::example(200).unwrap();
        example.validate(200, Geometry::Pads200).unwrap();
        assert!(example.validate(128, Geometry::Pads128).is_err());
        example.steps.pop();
        assert!(example.validate(200, Geometry::Pads200).is_err());
        let mut example = ContactScenario::example(128).unwrap();
        example.validate(128, Geometry::Pads128).unwrap();
        example.steps[1].at_us = 999999;
        assert!(example.validate(128, Geometry::Pads128).is_err());
    }
}
