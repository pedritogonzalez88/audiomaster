use crate::errors::DomainError;
use serde::{Deserialize, Serialize};

/// The kind of media an asset represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssetKind {
    Audio,
    Video,
}

/// Operational cue points, all expressed in milliseconds from the start of
/// the file. They are the contract between the library, the UI and the
/// playout engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CuePoints {
    /// First audible sample (leading silence removed).
    pub cue_in_ms: u32,
    /// End of the instrumental intro, where the voice usually starts.
    pub intro_ms: u32,
    /// Representative fragment used for previews/promos.
    pub hook_ms: u32,
    /// Point where the next track should start (manual or auto-detected).
    pub mix_point_ms: u32,
    /// Start of the fade-out envelope.
    pub fade_out_ms: u32,
    /// Last audible sample.
    pub cue_out_ms: u32,
}

impl CuePoints {
    /// Validates that cues are ordered and inside the file duration.
    pub fn validate(&self, duration_ms: u32) -> Result<(), DomainError> {
        if duration_ms == 0 {
            return Err(DomainError::InvalidDuration);
        }

        let ordered = [
            self.cue_in_ms,
            self.intro_ms,
            self.hook_ms,
            self.mix_point_ms,
            self.fade_out_ms,
            self.cue_out_ms,
        ];

        if ordered.windows(2).any(|pair| pair[0] > pair[1]) {
            return Err(DomainError::InvalidCuePoints(
                "cue_in <= intro <= hook <= mix_point <= fade_out <= cue_out".into(),
            ));
        }

        let effective_out = if self.cue_out_ms == 0 {
            duration_ms
        } else {
            self.cue_out_ms
        };

        if effective_out > duration_ms {
            return Err(DomainError::InvalidCuePoints(format!(
                "cue_out {} ms exceeds duration {} ms",
                effective_out, duration_ms
            )));
        }

        Ok(())
    }

    /// Duration that will actually be aired, excluding leading/trailing silence.
    pub fn effective_duration_ms(&self, duration_ms: u32) -> u32 {
        let start = self.cue_in_ms.min(duration_ms);
        let end = if self.cue_out_ms == 0 {
            duration_ms
        } else {
            self.cue_out_ms.min(duration_ms)
        };
        end.saturating_sub(start)
    }

    /// Default cues for a file with no analysis yet.
    pub fn whole_file(duration_ms: u32) -> Self {
        Self {
            cue_in_ms: 0,
            intro_ms: 0,
            hook_ms: 0,
            mix_point_ms: duration_ms,
            fade_out_ms: duration_ms,
            cue_out_ms: duration_ms,
        }
    }
}

/// A media asset known by the station library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaAsset {
    pub id: i64,
    pub category_id: i64,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub release_year: Option<i32>,
    pub bpm: u32,
    pub kind: AssetKind,
    pub file_path: String,
    pub checksum_sha256: String,
    pub duration_ms: u32,
    pub gain_db: f64,
    pub loudness_lufs: f64,
    pub cue_points: CuePoints,
    pub play_count: u64,
    pub last_played_at: Option<i64>,
}

impl MediaAsset {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.duration_ms == 0 {
            return Err(DomainError::InvalidDuration);
        }
        if self.title.trim().is_empty() {
            return Err(DomainError::NotPlayable("title is required".into()));
        }
        self.cue_points.validate(self.duration_ms)
    }
}

/// A broadcast station. v1 operates one active station per process, but the
/// model is multi-station from day one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Station {
    pub id: i64,
    pub name: String,
    pub timezone: String,
    pub is_active: bool,
}

/// How the engine should join two consecutive items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransitionKind {
    Crossfade,
    Cut,
    FadeIn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub kind: TransitionKind,
    pub duration_ms: u32,
}

impl Transition {
    pub const MAX_DURATION_MS: u32 = 30_000;

    pub fn crossfade(duration_ms: u32) -> Result<Self, DomainError> {
        if duration_ms > Self::MAX_DURATION_MS {
            return Err(DomainError::InvalidTransitionDuration {
                max_ms: Self::MAX_DURATION_MS,
                actual_ms: duration_ms,
            });
        }
        Ok(Self {
            kind: TransitionKind::Crossfade,
            duration_ms,
        })
    }
}

/// Playlist items may reference an asset, a live stream, a command or a
/// generated segment (HTH, hour marker...). This is why `asset_id` is optional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlaylistItemKind {
    Asset,
    Stream,
    Command,
    Generated,
    Relay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlaylistItemStatus {
    Pending,
    Played,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistItem {
    pub id: i64,
    pub position: u32,
    pub kind: PlaylistItemKind,
    pub asset_id: Option<i64>,
    pub stream_url: Option<String>,
    pub command: Option<String>,
    pub transition: Transition,
    pub status: PlaylistItemStatus,
}

impl PlaylistItem {
    pub fn from_asset(id: i64, position: u32, asset_id: i64, transition: Transition) -> Self {
        Self {
            id,
            position,
            kind: PlaylistItemKind::Asset,
            asset_id: Some(asset_id),
            stream_url: None,
            command: None,
            transition,
            status: PlaylistItemStatus::Pending,
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        match self.kind {
            PlaylistItemKind::Asset if self.asset_id.is_none() => {
                Err(DomainError::IncompletePlaylistItem("asset".into()))
            }
            PlaylistItemKind::Stream if self.stream_url.is_none() => {
                Err(DomainError::IncompletePlaylistItem("stream".into()))
            }
            PlaylistItemKind::Command if self.command.is_none() => {
                Err(DomainError::IncompletePlaylistItem("command".into()))
            }
            PlaylistItemKind::Generated | PlaylistItemKind::Relay => Ok(()),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: i64,
    pub station_id: i64,
    pub name: String,
    pub items: Vec<PlaylistItem>,
}

impl Playlist {
    pub fn total_duration_ms(&self, asset_duration: impl Fn(i64) -> Option<u32>) -> u32 {
        self.items
            .iter()
            .filter_map(|item| item.asset_id.and_then(&asset_duration))
            .sum()
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        for item in &self.items {
            item.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_points_validate_ordered() {
        let cues = CuePoints {
            cue_in_ms: 0,
            intro_ms: 1000,
            hook_ms: 2000,
            mix_point_ms: 3000,
            fade_out_ms: 4000,
            cue_out_ms: 5000,
        };
        assert!(cues.validate(5000).is_ok());
        assert!(cues.validate(4000).is_err());
    }

    #[test]
    fn cue_points_reject_unordered() {
        let cues = CuePoints {
            cue_in_ms: 0,
            intro_ms: 2000,
            hook_ms: 1000,
            mix_point_ms: 3000,
            fade_out_ms: 4000,
            cue_out_ms: 5000,
        };
        assert!(cues.validate(5000).is_err());
    }

    #[test]
    fn cue_points_zero_duration_rejected() {
        assert!(CuePoints::default().validate(0).is_err());
    }

    #[test]
    fn cue_points_zero_cue_out_uses_duration() {
        let cues = CuePoints {
            cue_in_ms: 0,
            intro_ms: 0,
            hook_ms: 0,
            mix_point_ms: 0,
            fade_out_ms: 0,
            cue_out_ms: 0,
        };
        assert!(cues.validate(1000).is_ok());
        assert_eq!(cues.effective_duration_ms(1000), 1000);
    }

    #[test]
    fn cue_points_effective_duration() {
        let cues = CuePoints {
            cue_in_ms: 500,
            intro_ms: 500,
            hook_ms: 500,
            mix_point_ms: 1500,
            fade_out_ms: 1500,
            cue_out_ms: 2000,
        };
        assert_eq!(cues.effective_duration_ms(2000), 1500);
    }

    #[test]
    fn cue_points_whole_file() {
        let cues = CuePoints::whole_file(10_000);
        assert!(cues.validate(10_000).is_ok());
        assert_eq!(cues.mix_point_ms, 10_000);
        assert_eq!(cues.cue_out_ms, 10_000);
    }

    #[test]
    fn media_asset_validate_ok() {
        let asset = MediaAsset {
            id: 1,
            category_id: 1,
            title: "Title".into(),
            artist: "Artist".into(),
            album: None,
            release_year: None,
            bpm: 120,
            kind: AssetKind::Audio,
            file_path: "C:/x.mp3".into(),
            checksum_sha256: "abc".into(),
            duration_ms: 1000,
            gain_db: 0.0,
            loudness_lufs: -23.0,
            cue_points: CuePoints::whole_file(1000),
            play_count: 0,
            last_played_at: None,
        };
        assert!(asset.validate().is_ok());
    }

    #[test]
    fn media_asset_rejects_empty_title() {
        let mut asset = MediaAsset {
            id: 1,
            category_id: 1,
            title: "  ".into(),
            artist: "A".into(),
            album: None,
            release_year: None,
            bpm: 0,
            kind: AssetKind::Audio,
            file_path: "x".into(),
            checksum_sha256: "c".into(),
            duration_ms: 1000,
            gain_db: 0.0,
            loudness_lufs: 0.0,
            cue_points: CuePoints::whole_file(1000),
            play_count: 0,
            last_played_at: None,
        };
        assert!(asset.validate().is_err());
        asset.title = "T".into();
        asset.duration_ms = 0;
        assert!(asset.validate().is_err());
    }

    #[test]
    fn transition_crossfade_max() {
        assert!(Transition::crossfade(1000).is_ok());
        assert!(Transition::crossfade(30_000).is_ok());
        assert!(Transition::crossfade(30_001).is_err());
    }

    #[test]
    fn playlist_item_validate() {
        let t = Transition::crossfade(1000).unwrap();
        let asset_item = PlaylistItem::from_asset(1, 0, 5, t);
        assert!(asset_item.validate().is_ok());

        let stream_item = PlaylistItem {
            id: 2,
            position: 1,
            kind: PlaylistItemKind::Stream,
            asset_id: None,
            stream_url: None,
            command: None,
            transition: t,
            status: PlaylistItemStatus::Pending,
        };
        assert!(stream_item.validate().is_err());
    }

    #[test]
    fn playlist_total_duration() {
        let t = Transition::crossfade(0).unwrap();
        let playlist = Playlist {
            id: 1,
            station_id: 1,
            name: "P".into(),
            items: vec![
                PlaylistItem::from_asset(1, 0, 10, t),
                PlaylistItem::from_asset(2, 1, 20, t),
            ],
        };
        let durations = |id: i64| if id == 10 { Some(1000) } else { Some(2000) };
        assert_eq!(playlist.total_duration_ms(durations), 3000);
        assert!(playlist.validate().is_ok());
    }

    #[test]
    fn asset_kind_serialization() {
        assert_eq!(
            serde_json::to_string(&AssetKind::Audio).unwrap(),
            "\"AUDIO\""
        );
        assert_eq!(
            serde_json::to_string(&AssetKind::Video).unwrap(),
            "\"VIDEO\""
        );
    }
}
