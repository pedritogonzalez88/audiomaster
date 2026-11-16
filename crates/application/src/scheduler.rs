use domain::{ScheduleEvent, ScheduleStatus};
use std::collections::VecDeque;

/// In-memory scheduler runtime for Fase 3.
///
/// Evaluates a set of scheduled events against a virtual clock and reports the
/// ones that must fire. Persistence and DB-backed repositories arrive in the
/// next increment; this runtime is what makes the state machine testable.
pub struct SchedulerRuntime {
    events: Vec<ScheduleEvent>,
    lookahead_ms: i64,
    fired: VecDeque<i64>,
}

impl SchedulerRuntime {
    pub fn new(lookahead_ms: i64) -> Self {
        Self {
            events: Vec::new(),
            lookahead_ms,
            fired: VecDeque::new(),
        }
    }

    pub fn add_event(&mut self, event: ScheduleEvent) {
        self.events.push(event);
    }

    pub fn events(&self) -> &[ScheduleEvent] {
        &self.events
    }

    /// Advances the scheduler to `now_ms` and returns the events that must fire.
    pub fn evaluate(&mut self, now_ms: i64) -> Vec<ScheduleEvent> {
        let mut to_fire = Vec::new();

        for event in self.events.iter_mut() {
            if !event.is_actionable() {
                continue;
            }

            // Mark missed events.
            if let Some(end) = event.end_utc_ms {
                if now_ms > end && event.status == ScheduleStatus::Scheduled {
                    event.status = ScheduleStatus::Missed;
                    continue;
                }
            }

            // Arm events within the lookahead window.
            if event.status == ScheduleStatus::Scheduled
                && event.start_utc_ms > now_ms
                && event.start_utc_ms - now_ms <= self.lookahead_ms
            {
                event.status = ScheduleStatus::Armed;
            }

            // Fire armed/scheduled events whose time has come.
            if event.start_utc_ms <= now_ms
                && event.in_window(now_ms)
                && !self.fired.contains(&event.id)
            {
                event.status = ScheduleStatus::Fired;
                self.fired.push_back(event.id);
                to_fire.push(event.clone());
            }
        }

        // Keep the fired list bounded.
        while self.fired.len() > 1024 {
            self.fired.pop_front();
        }

        to_fire
    }

    /// Marks an event as done (called after the playout engine acknowledges it).
    pub fn mark_done(&mut self, id: i64) {
        if let Some(event) = self.events.iter_mut().find(|e| e.id == id) {
            event.status = ScheduleStatus::Done;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{ScheduleActionKind, ScheduleEventKind, ScheduleStatus};

    fn event(id: i64, start: i64, end: Option<i64>) -> ScheduleEvent {
        ScheduleEvent {
            id,
            station_id: 1,
            kind: ScheduleEventKind::At,
            action: ScheduleActionKind::PlayAsset,
            asset_id: Some(10),
            playlist_id: None,
            command: None,
            priority: 100,
            daypart_id: None,
            start_utc_ms: start,
            end_utc_ms: end,
            status: ScheduleStatus::Scheduled,
        }
    }

    #[test]
    fn fires_event_at_its_time() {
        let mut rt = SchedulerRuntime::new(30_000);
        rt.add_event(event(1, 1000, Some(2000)));
        let fired = rt.evaluate(1000);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].id, 1);
        assert_eq!(rt.events()[0].status, ScheduleStatus::Fired);
    }

    #[test]
    fn arms_event_within_lookahead() {
        let mut rt = SchedulerRuntime::new(30_000);
        rt.add_event(event(1, 5000, None));
        let fired = rt.evaluate(1000);
        assert!(fired.is_empty());
        assert_eq!(rt.events()[0].status, ScheduleStatus::Armed);
    }

    #[test]
    fn marks_missed_event() {
        let mut rt = SchedulerRuntime::new(30_000);
        rt.add_event(event(1, 1000, Some(1500)));
        let fired = rt.evaluate(2000);
        assert!(fired.is_empty());
        assert_eq!(rt.events()[0].status, ScheduleStatus::Missed);
    }

    #[test]
    fn does_not_fire_twice() {
        let mut rt = SchedulerRuntime::new(30_000);
        rt.add_event(event(1, 1000, None));
        assert_eq!(rt.evaluate(1000).len(), 1);
        assert_eq!(rt.evaluate(1000).len(), 0);
        assert_eq!(rt.evaluate(1001).len(), 0);
    }

    #[test]
    fn mark_done() {
        let mut rt = SchedulerRuntime::new(30_000);
        rt.add_event(event(1, 1000, None));
        rt.evaluate(1000);
        rt.mark_done(1);
        assert_eq!(rt.events()[0].status, ScheduleStatus::Done);
    }
}
