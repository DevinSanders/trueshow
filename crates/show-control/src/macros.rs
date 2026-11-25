//! Reusable multi-action Macros / Automations triggered from slides, hotkeys, or external cues.

use crate::timers::TimerId;
use core_scene::{LayerKind, LookId, OutputId, SceneDiff, SlideId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MacroError {
    #[error("macro not found: {0}")]
    NotFound(Uuid),
    #[error("macro recursion cycle detected involving {0}")]
    CycleDetected(Uuid),
    #[error("macro name cannot be empty")]
    EmptyName,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MacroAction {
    ApplyScene(Box<SceneDiff>),
    SwitchLook {
        output_id: OutputId,
        look_id: LookId,
    },
    TriggerSlide {
        slide_id: SlideId,
    },
    ClearLayer(LayerKind),
    StartTimer(TimerId),
    PauseTimer(TimerId),
    ResetTimer(TimerId),
    SendMidiRaw(Vec<u8>),
    SendOsc {
        address: String,
        string_arg: Option<String>,
    },
    InvokeMacro(Uuid),
    DelayMs(u32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroDefinition {
    pub id: Uuid,
    pub name: String,
    pub hotkey: Option<String>,
    pub actions: Vec<MacroAction>,
}

/// Flattened, cycle-checked execution plan ready for the engine runtime.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroExecutionPlan {
    pub macro_id: Uuid,
    pub steps: Vec<MacroAction>,
}

#[derive(Default)]
pub struct MacroRegistry {
    macros: BTreeMap<Uuid, MacroDefinition>,
}

impl MacroRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&mut self, def: MacroDefinition) -> Result<(), MacroError> {
        if def.name.trim().is_empty() {
            return Err(MacroError::EmptyName);
        }
        self.macros.insert(def.id, def);
        Ok(())
    }

    pub fn list(&self) -> Vec<MacroDefinition> {
        self.macros.values().cloned().collect()
    }

    pub fn find_by_hotkey(&self, hotkey: &str) -> Option<&MacroDefinition> {
        self.macros.values().find(|m| {
            m.hotkey
                .as_deref()
                .is_some_and(|h| h.eq_ignore_ascii_case(hotkey))
        })
    }

    /// Expands nested `InvokeMacro` calls into a flat execution plan, rejecting recursive cycles.
    pub fn resolve_plan(&self, id: Uuid) -> Result<MacroExecutionPlan, MacroError> {
        let mut visiting = HashSet::new();
        let mut steps = Vec::new();
        self.expand(id, &mut visiting, &mut steps)?;
        Ok(MacroExecutionPlan {
            macro_id: id,
            steps,
        })
    }

    fn expand(
        &self,
        id: Uuid,
        visiting: &mut HashSet<Uuid>,
        out: &mut Vec<MacroAction>,
    ) -> Result<(), MacroError> {
        if !visiting.insert(id) {
            return Err(MacroError::CycleDetected(id));
        }
        let def = self.macros.get(&id).ok_or(MacroError::NotFound(id))?;
        for action in &def.actions {
            match action {
                MacroAction::InvokeMacro(child) => self.expand(*child, visiting, out)?,
                MacroAction::SwitchLook { output_id, look_id } => {
                    out.push(MacroAction::ApplyScene(Box::new(
                        SceneDiff::SetOutputLook {
                            output_id: *output_id,
                            look_id: *look_id,
                        },
                    )));
                }
                MacroAction::TriggerSlide { slide_id } => {
                    out.push(MacroAction::ApplyScene(Box::new(SceneDiff::TriggerSlide {
                        slide_id: *slide_id,
                        transition_override: None,
                    })));
                }
                MacroAction::ClearLayer(layer) => {
                    out.push(MacroAction::ApplyScene(Box::new(SceneDiff::ClearLayer(
                        *layer,
                    ))));
                }
                other => out.push(other.clone()),
            }
        }
        visiting.remove(&id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_scene::{LOOK_BROADCAST_LOWER_THIRD, OUTPUT_AUDIENCE_MAIN};

    #[test]
    fn resolves_nested_macros_and_detects_cycles_and_missing_ids() {
        let mut reg = MacroRegistry::new();
        let id_child = Uuid::from_u128(1);
        let id_parent = Uuid::from_u128(2);

        // Failure path: empty name
        assert_eq!(
            reg.upsert(MacroDefinition {
                id: id_child,
                name: "   ".into(),
                hotkey: None,
                actions: vec![],
            }),
            Err(MacroError::EmptyName)
        );

        reg.upsert(MacroDefinition {
            id: id_child,
            name: "Broadcast Lower Third".into(),
            hotkey: Some("Ctrl+Shift+L".into()),
            actions: vec![
                MacroAction::SwitchLook {
                    output_id: OUTPUT_AUDIENCE_MAIN,
                    look_id: LOOK_BROADCAST_LOWER_THIRD,
                },
                MacroAction::SendOsc {
                    address: "/atem/dsk/1/on".into(),
                    string_arg: None,
                },
            ],
        })
        .expect("upsert child");

        reg.upsert(MacroDefinition {
            id: id_parent,
            name: "Sermon Start".into(),
            hotkey: Some("F5".into()),
            actions: vec![
                MacroAction::ClearLayer(LayerKind::MediaBackground),
                MacroAction::TriggerSlide {
                    slide_id: SlideId::from_u128(1003),
                },
                MacroAction::InvokeMacro(id_child),
                MacroAction::SendMidiRaw(vec![0x90, 60, 127]),
            ],
        })
        .expect("upsert parent");

        assert_eq!(reg.list().len(), 2);
        assert_eq!(reg.find_by_hotkey("f5").map(|m| m.id), Some(id_parent));

        let plan = reg.resolve_plan(id_parent).expect("resolve");
        assert_eq!(plan.steps.len(), 5);

        // Failure path: missing macro ID
        assert!(matches!(
            reg.resolve_plan(Uuid::from_u128(999)),
            Err(MacroError::NotFound(_))
        ));

        // Failure path: recursive cycle detected
        reg.upsert(MacroDefinition {
            id: id_child,
            name: "Cycle".into(),
            hotkey: None,
            actions: vec![MacroAction::InvokeMacro(id_parent)],
        })
        .expect("upsert cycle");
        assert!(matches!(
            reg.resolve_plan(id_parent),
            Err(MacroError::CycleDetected(_))
        ));
    }
}
