use serde::{Deserialize, Serialize};

/// Kind of trigger that fires a scheduled event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScheduleEventKind {
    At,
    Recurring,
    Dtmf,
    Manual,
    Api,
}

/// What the event does when it fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScheduleActionKind {
    PlayAsset,
    PlayPlaylist,
    PlayCommand,
    StartRelay,
    StopCurrent,
    InsertBreak,
}

/// Lifecycle state of a scheduled event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScheduleStatus {
    Scheduled,
    Armed,
    Fired,
    Done,
    Missed,
    Skipped,
}

/// A scheduled event. `start_utc_ms` is the instant at which it should fire;
/// `end_utc_ms` is the end of its firing window (events not fired by then are
/// marked `Missed`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleEvent {
    pub id: i64,
    pub station_id: i64,
    pub kind: ScheduleEventKind,
    pub action: ScheduleActionKind,
    pub asset_id: Option<i64>,
    pub playlist_id: Option<i64>,
    pub command: Option<String>,
    pub priority: i32,
    pub daypart_id: Option<i64>,
    pub start_utc_ms: i64,
    pub end_utc_ms: Option<i64>,
    pub status: ScheduleStatus,
}

impl ScheduleEvent {
    pub fn is_actionable(&self) -> bool {
        matches!(
            self.status,
            ScheduleStatus::Scheduled | ScheduleStatus::Armed
        )
    }

    pub fn in_window(&self, now_ms: i64) -> bool {
        if now_ms < self.start_utc_ms {
            return false;
        }
        match self.end_utc_ms {
            Some(end) => now_ms <= end,
            None => true,
        }
    }
}

/// Recurrence rule for a `Recurring` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recurrence {
    /// Bitmask of active days (bit 0 = Monday .. bit 6 = Sunday).
    pub days_of_week: u8,
    /// Milliseconds from midnight at which the event fires.
    pub time_of_day_ms: u32,
    /// Repeat every N days (1 = daily).
    pub interval_days: u32,
}

impl Recurrence {
    pub fn is_active_on(&self, weekday: u8) -> bool {
        (self.days_of_week & (1 << weekday)) != 0
    }
}

/// A time window inside a day used to group scheduling rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Daypart {
    pub id: i64,
    pub station_id: i64,
    pub name: String,
    pub start_ms: u32,
    pub end_ms: u32,
}

impl Daypart {
    pub fn contains_ms(&self, ms_from_midnight: u32) -> bool {
        if self.start_ms <= self.end_ms {
            ms_from_midnight >= self.start_ms && ms_from_midnight < self.end_ms
        } else {
            // Wraps midnight.
            ms_from_midnight >= self.start_ms || ms_from_midnight < self.end_ms
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_window() {
        let event = ScheduleEvent {
            id: 1,
            station_id: 1,
            kind: ScheduleEventKind::At,
            action: ScheduleActionKind::PlayAsset,
            asset_id: Some(10),
            playlist_id: None,
            command: None,
            priority: 0,
            daypart_id: None,
            start_utc_ms: 1000,
            end_utc_ms: Some(2000),
            status: ScheduleStatus::Scheduled,
        };
        assert!(!event.in_window(999));
        assert!(event.in_window(1000));
        assert!(event.in_window(2000));
        assert!(!event.in_window(2001));
        assert!(event.is_actionable());
    }

    #[test]
    fn recurrence_weekday_bitmask() {
        let r = Recurrence {
            days_of_week: 0b001_1111, // Mon-Fri
            time_of_day_ms: 0,
            interval_days: 1,
        };
        assert!(r.is_active_on(0)); // Monday
        assert!(r.is_active_on(4)); // Friday
        assert!(!r.is_active_on(5)); // Saturday
        assert!(!r.is_active_on(6)); // Sunday
    }

    #[test]
    fn daypart_wraps_midnight() {
        let dp = Daypart {
            id: 1,
            station_id: 1,
            name: "Night".into(),
            start_ms: 22 * 3600 * 1000,
            end_ms: 6 * 3600 * 1000,
        };
        assert!(dp.contains_ms(23 * 3600 * 1000));
        assert!(dp.contains_ms(3 * 3600 * 1000));
        assert!(!dp.contains_ms(12 * 3600 * 1000));
    }

    #[test]
    fn missed_status_not_actionable() {
        let mut event = ScheduleEvent {
            id: 2,
            station_id: 1,
            kind: ScheduleEventKind::Manual,
            action: ScheduleActionKind::StopCurrent,
            asset_id: None,
            playlist_id: None,
            command: None,
            priority: 0,
            daypart_id: None,
            start_utc_ms: 0,
            end_utc_ms: None,
            status: ScheduleStatus::Missed,
        };
        assert!(!event.is_actionable());
        event.status = ScheduleStatus::Scheduled;
        assert!(event.is_actionable());
    }
}
