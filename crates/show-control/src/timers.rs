//! Concurrent Countdown, Count-Up, and Elapsed timers routable to Audience and Stage displays.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TimerId(pub Uuid);

impl TimerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TimerId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimerKind {
    Countdown {
        initial_secs: i64,
        allow_overrun: bool,
    },
    CountUp,
    Elapsed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimerSnapshot {
    pub id: TimerId,
    pub name: String,
    pub kind: TimerKind,
    pub running: bool,
    pub current_secs: i64,
    pub overrun: bool,
    pub formatted: String,
}

struct TimerEntry {
    id: TimerId,
    name: String,
    kind: TimerKind,
    running: bool,
    current_ms: i64,
}

impl TimerEntry {
    fn snapshot(&self) -> TimerSnapshot {
        let secs = self.current_ms / 1000;
        let overrun = matches!(self.kind, TimerKind::Countdown { .. }) && self.current_ms < 0;
        TimerSnapshot {
            id: self.id,
            name: self.name.clone(),
            kind: self.kind,
            running: self.running,
            current_secs: secs,
            overrun,
            formatted: format_timer_secs(secs),
        }
    }
}

/// Formats seconds as `MM:SS` or `HH:MM:SS` (with leading `-` on overrun).
pub fn format_timer_secs(total_secs: i64) -> String {
    let neg = total_secs < 0;
    let abs = total_secs.unsigned_abs();
    let hours = abs / 3600;
    let mins = (abs % 3600) / 60;
    let secs = abs % 60;
    let sign = if neg { "-" } else { "" };
    if hours > 0 {
        format!("{sign}{hours:02}:{mins:02}:{secs:02}")
    } else {
        format!("{sign}{mins:02}:{secs:02}")
    }
}

/// Manages unlimited concurrent timers with deterministic millisecond ticking.
#[derive(Default)]
pub struct TimerEngine {
    timers: BTreeMap<TimerId, TimerEntry>,
}

impl TimerEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&mut self, name: impl Into<String>, kind: TimerKind) -> TimerId {
        let id = TimerId::new();
        let initial_ms = match kind {
            TimerKind::Countdown { initial_secs, .. } => initial_secs.max(0) * 1000,
            TimerKind::CountUp | TimerKind::Elapsed => 0,
        };
        self.timers.insert(
            id,
            TimerEntry {
                id,
                name: name.into(),
                kind,
                running: false,
                current_ms: initial_ms,
            },
        );
        id
    }

    pub fn start(&mut self, id: TimerId) -> bool {
        if let Some(t) = self.timers.get_mut(&id) {
            t.running = true;
            true
        } else {
            false
        }
    }

    pub fn pause(&mut self, id: TimerId) -> bool {
        if let Some(t) = self.timers.get_mut(&id) {
            t.running = false;
            true
        } else {
            false
        }
    }

    pub fn reset(&mut self, id: TimerId) -> bool {
        if let Some(t) = self.timers.get_mut(&id) {
            t.running = false;
            t.current_ms = match t.kind {
                TimerKind::Countdown { initial_secs, .. } => initial_secs.max(0) * 1000,
                TimerKind::CountUp | TimerKind::Elapsed => 0,
            };
            true
        } else {
            false
        }
    }

    pub fn add_seconds(&mut self, id: TimerId, delta_secs: i64) -> bool {
        if let Some(t) = self.timers.get_mut(&id) {
            t.current_ms += delta_secs * 1000;
            true
        } else {
            false
        }
    }

    pub fn tick_ms(&mut self, elapsed_ms: u64) {
        let dt = elapsed_ms as i64;
        for t in self.timers.values_mut() {
            if !t.running {
                continue;
            }
            match t.kind {
                TimerKind::Countdown { allow_overrun, .. } => {
                    t.current_ms -= dt;
                    if !allow_overrun && t.current_ms <= 0 {
                        t.current_ms = 0;
                        t.running = false;
                    }
                }
                TimerKind::CountUp | TimerKind::Elapsed => {
                    t.current_ms += dt;
                }
            }
        }
    }

    pub fn get(&self, id: TimerId) -> Option<TimerSnapshot> {
        self.timers.get(&id).map(TimerEntry::snapshot)
    }

    pub fn list(&self) -> Vec<TimerSnapshot> {
        self.timers.values().map(TimerEntry::snapshot).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drives_concurrent_countdown_overrun_and_elapsed_timers() {
        let mut engine = TimerEngine::new();
        let sermon = engine.create(
            "Sermon",
            TimerKind::Countdown {
                initial_secs: 5,
                allow_overrun: true,
            },
        );
        let prelude = engine.create(
            "Prelude",
            TimerKind::Countdown {
                initial_secs: 3,
                allow_overrun: false,
            },
        );
        let service = engine.create("Service Elapsed", TimerKind::Elapsed);

        assert!(engine.start(sermon));
        assert!(engine.start(prelude));
        assert!(engine.start(service));
        assert!(!engine.start(TimerId::default()));
        assert!(!engine.pause(TimerId::default()));
        assert!(!engine.reset(TimerId::default()));
        assert!(!engine.add_seconds(TimerId::default(), 10));

        engine.tick_ms(4000);
        assert_eq!(engine.get(sermon).expect("sermon").formatted, "00:01");
        assert_eq!(engine.get(prelude).expect("prelude").formatted, "00:00");
        assert!(!engine.get(prelude).expect("prelude").running);
        assert_eq!(engine.get(service).expect("service").formatted, "00:04");

        // Overrun past 00:00
        engine.tick_ms(3000);
        let s = engine.get(sermon).expect("sermon overrun");
        assert!(s.overrun);
        assert_eq!(s.formatted, "-00:02");

        // Add 3700 seconds to test HH:MM:SS formatting + pause + reset
        assert!(engine.add_seconds(service, 3600));
        assert_eq!(engine.get(service).expect("hours").formatted, "01:00:07");
        assert!(engine.pause(sermon));
        assert!(engine.reset(sermon));
        assert_eq!(engine.get(sermon).expect("reset").formatted, "00:05");
        assert_eq!(engine.list().len(), 3);
    }
}
