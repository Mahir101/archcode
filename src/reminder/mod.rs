use std::sync::{Arc, Mutex};

/// Schedule kinds for reminders.
#[derive(Clone)]
pub enum ScheduleKind {
    /// Fire once on the first eligible turn.
    OneShot,
    /// Fire every N turns.
    Turn { interval: usize },
    /// Fire when a condition is true (up to max_fires times).
    Condition {
        max_fires: usize,
        condition: Arc<dyn Fn(&ConversationState) -> bool + Send + Sync>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct ConversationState {
    pub turn: usize,
    pub message_count: usize,
}

#[derive(Clone)]
pub struct Reminder {
    pub id: String,
    pub content: String,
    pub schedule: ScheduleKind,
    pub priority: i32,
    pub active: bool,
    fires: Arc<Mutex<usize>>,
    last_turn: Arc<Mutex<usize>>,
}

impl Reminder {
    pub fn new(
        id: impl Into<String>,
        content: impl Into<String>,
        schedule: ScheduleKind,
        priority: i32,
    ) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            schedule,
            priority,
            active: true,
            fires: Arc::new(Mutex::new(0)),
            last_turn: Arc::new(Mutex::new(0)),
        }
    }

    pub fn should_fire(&self, state: &ConversationState) -> bool {
        if !self.active {
            return false;
        }
        match &self.schedule {
            ScheduleKind::OneShot => {
                let fires = *self.fires.lock().unwrap();
                fires == 0
            }
            ScheduleKind::Turn { interval } => {
                let last = *self.last_turn.lock().unwrap();
                state.turn > 0 && (state.turn - last) >= *interval
            }
            ScheduleKind::Condition {
                max_fires,
                condition,
            } => {
                let fires = *self.fires.lock().unwrap();
                fires < *max_fires && condition(state)
            }
        }
    }

    pub fn mark_fired(&self, turn: usize) {
        *self.fires.lock().unwrap() += 1;
        *self.last_turn.lock().unwrap() = turn;
    }
}

pub struct ReminderManager {
    reminders: Vec<Reminder>,
}

impl ReminderManager {
    pub fn new() -> Self {
        Self { reminders: vec![] }
    }

    pub fn register(&mut self, r: Reminder) {
        self.reminders.push(r);
    }

    /// Collect all reminders that should fire this turn, sorted by priority.
    pub fn collect(&self, state: &ConversationState) -> Vec<String> {
        let mut eligible: Vec<&Reminder> = self
            .reminders
            .iter()
            .filter(|r| r.should_fire(state))
            .collect();

        eligible.sort_by_key(|r| r.priority);

        let mut out = vec![];
        for r in eligible {
            out.push(r.content.clone());
            r.mark_fired(state.turn);
        }
        out
    }

    /// Inject reminder content as a <system-reminder> block into the last user message context.
    pub fn inject(&self, state: &ConversationState) -> Option<String> {
        let parts = self.collect(state);
        if parts.is_empty() {
            return None;
        }
        Some(format!(
            "<system-reminder>\n{}\n</system-reminder>",
            parts.join("\n\n")
        ))
    }
}

impl Default for ReminderManager {
    fn default() -> Self {
        Self::new()
    }
}
