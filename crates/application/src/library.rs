use crate::errors::{AppError, AppResult};
use domain::{DomainError, MediaAsset};

#[derive(Debug)]
pub struct Category {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub color_hex: String,
    pub is_commercial: bool,
}

/// Metadata extracted from a media file by an infrastructure probe.
#[derive(Debug, Clone)]
pub struct ProbedMedia {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub release_year: Option<i32>,
    pub bpm: u32,
    pub kind: domain::AssetKind,
    pub duration_ms: u32,
    pub checksum_sha256: String,
    pub loudness_lufs: f64,
    pub cue_points: domain::CuePoints,
}

/// Filters accepted by the library search.
#[derive(Debug, Clone, Default)]
pub struct LibraryQuery {
    pub text: String,
    pub category_id: Option<i64>,
    pub limit: u32,
    pub offset: u32,
}

impl LibraryQuery {
    pub fn sanitized(mut self) -> Self {
        self.text = self.text.trim().to_string();
        self.limit = self.limit.clamp(1, 200);
        self
    }
}

/// Outcome of a folder import.
#[derive(Debug, Clone, Default)]
pub struct ImportSummary {
    pub imported: usize,
    pub skipped: usize,
    pub failed: Vec<(std::path::PathBuf, String)>,
}

impl ImportSummary {
    pub fn total(&self) -> usize {
        self.imported + self.skipped + self.failed.len()
    }
}

/// Imports files into the library and searches it.
pub struct LibraryService {
    repository: std::sync::Arc<dyn crate::ports::MediaRepository>,
    probe: std::sync::Arc<dyn crate::ports::MediaProbe>,
}

impl LibraryService {
    pub fn new(
        repository: std::sync::Arc<dyn crate::ports::MediaRepository>,
        probe: std::sync::Arc<dyn crate::ports::MediaProbe>,
    ) -> Self {
        Self { repository, probe }
    }

    /// Imports a file. Duplicate checksums are idempotent: the existing asset
    /// is returned and no new row is created.
    pub fn import_path(&self, path: &std::path::Path, category_id: i64) -> AppResult<MediaAsset> {
        self.import_path_with_status(path, category_id)
            .map(|(asset, _)| asset)
    }

    /// Like `import_path`, but reports whether a new row was created.
    pub fn import_path_with_status(
        &self,
        path: &std::path::Path,
        category_id: i64,
    ) -> AppResult<(MediaAsset, bool)> {
        if !path.is_file() {
            return Err(AppError::InvalidInput(format!(
                "file does not exist: {}",
                path.display()
            )));
        }

        let probed = self.probe.probe(path)?;
        if let Some(existing) = self.repository.find_by_checksum(&probed.checksum_sha256)? {
            return Ok((existing, false));
        }

        let asset = self.repository.insert(category_id, &probed, path)?;
        asset.validate().map_err(AppError::Domain)?;
        Ok((asset, true))
    }

    /// Recursively imports every supported audio file under `root`.
    pub fn import_folder(
        &self,
        root: &std::path::Path,
        category_id: i64,
        recursive: bool,
    ) -> AppResult<ImportSummary> {
        if !root.is_dir() {
            return Err(AppError::InvalidInput(format!(
                "directory does not exist: {}",
                root.display()
            )));
        }

        let mut summary = ImportSummary::default();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let entries = std::fs::read_dir(&dir).map_err(|error| {
                AppError::InvalidInput(format!("read {}: {error}", dir.display()))
            })?;
            for entry in entries.flatten() {
                let path = entry.path();
                let file_type = entry.file_type().map_err(|error| {
                    AppError::InvalidInput(format!("stat {}: {error}", path.display()))
                })?;
                if file_type.is_dir() {
                    if recursive {
                        stack.push(path);
                    }
                    continue;
                }
                if !file_type.is_file() || !is_supported_audio(&path) {
                    continue;
                }
                match self.import_path_with_status(&path, category_id) {
                    Ok((_, true)) => summary.imported += 1,
                    Ok((_, false)) => summary.skipped += 1,
                    Err(error) => summary.failed.push((path, error.to_string())),
                }
            }
        }

        Ok(summary)
    }

    pub fn search(&self, query: LibraryQuery) -> AppResult<Vec<MediaAsset>> {
        self.repository.search(&query.sanitized())
    }

    pub fn categories(&self) -> AppResult<Vec<Category>> {
        self.repository.categories()
    }

    pub fn get(&self, id: i64) -> AppResult<MediaAsset> {
        self.repository
            .get(id)?
            .ok_or_else(|| AppError::NotFound(format!("asset {id}")))
    }
}

/// Validates domain invariants before persisting, so adapters can stay dumb.
pub fn validate_asset(asset: &MediaAsset) -> AppResult<()> {
    asset
        .validate()
        .map_err(|error: DomainError| AppError::Domain(error))
}

fn is_supported_audio(path: &std::path::Path) -> bool {
    const EXTENSIONS: &[&str] = &[
        "mp3", "flac", "wav", "ogg", "oga", "opus", "aac", "m4a", "mp4", "mkv", "avi", "mov",
        "webm",
    ];
    path.extension()
        .and_then(|value| value.to_str())
        .map(|ext| EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{MediaProbe, MediaRepository};
    use domain::{AssetKind, CuePoints};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeRepo {
        assets: Mutex<Vec<MediaAsset>>,
    }

    impl MediaRepository for FakeRepo {
        fn insert(
            &self,
            category_id: i64,
            probed: &ProbedMedia,
            file_path: &std::path::Path,
        ) -> AppResult<MediaAsset> {
            let mut assets = self.assets.lock().unwrap();
            let asset = MediaAsset {
                id: assets.len() as i64 + 1,
                category_id,
                title: probed.title.clone(),
                artist: probed.artist.clone(),
                album: probed.album.clone(),
                release_year: probed.release_year,
                bpm: probed.bpm,
                kind: probed.kind,
                file_path: file_path.display().to_string(),
                checksum_sha256: probed.checksum_sha256.clone(),
                duration_ms: probed.duration_ms,
                gain_db: 0.0,
                loudness_lufs: probed.loudness_lufs,
                cue_points: probed.cue_points,
                play_count: 0,
                last_played_at: None,
            };
            assets.push(asset.clone());
            Ok(asset)
        }

        fn find_by_checksum(&self, checksum: &str) -> AppResult<Option<MediaAsset>> {
            Ok(self
                .assets
                .lock()
                .unwrap()
                .iter()
                .find(|asset| asset.checksum_sha256 == checksum)
                .cloned())
        }

        fn search(&self, query: &LibraryQuery) -> AppResult<Vec<MediaAsset>> {
            let needle = query.text.to_lowercase();
            Ok(self
                .assets
                .lock()
                .unwrap()
                .iter()
                .filter(|asset| {
                    needle.is_empty()
                        || asset.title.to_lowercase().contains(&needle)
                        || asset.artist.to_lowercase().contains(&needle)
                })
                .take(query.limit as usize)
                .cloned()
                .collect())
        }

        fn get(&self, id: i64) -> AppResult<Option<MediaAsset>> {
            Ok(self
                .assets
                .lock()
                .unwrap()
                .iter()
                .find(|asset| asset.id == id)
                .cloned())
        }

        fn categories(&self) -> AppResult<Vec<Category>> {
            Ok(vec![Category {
                id: 1,
                code: "MUSIC".into(),
                name: "Música".into(),
                color_hex: "#3b82f6".into(),
                is_commercial: false,
            }])
        }
    }

    struct FakeProbe;

    impl MediaProbe for FakeProbe {
        fn probe(&self, path: &std::path::Path) -> AppResult<ProbedMedia> {
            Ok(ProbedMedia {
                title: path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("Unknown")
                    .into(),
                artist: "Test Artist".into(),
                album: None,
                release_year: None,
                bpm: 120,
                kind: AssetKind::Audio,
                duration_ms: 180_000,
                checksum_sha256: path.display().to_string(),
                loudness_lufs: -23.0,
                cue_points: CuePoints::whole_file(180_000),
            })
        }
    }

    #[test]
    fn import_is_idempotent_by_checksum() {
        let repo = Arc::new(FakeRepo::default());
        let service = LibraryService::new(repo.clone(), Arc::new(FakeProbe));
        let path = std::env::current_exe().unwrap();

        let first = service.import_path(&path, 1).unwrap();
        let second = service.import_path(&path, 1).unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(repo.assets.lock().unwrap().len(), 1);
    }

    #[test]
    fn search_filters_by_text_and_limits() {
        let repo = Arc::new(FakeRepo::default());
        let service = LibraryService::new(repo, Arc::new(FakeProbe));
        let path = std::env::current_exe().unwrap();
        service.import_path(&path, 1).unwrap();

        let results = service
            .search(LibraryQuery {
                text: "test".into(),
                limit: 50,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].artist, "Test Artist");
    }

    #[test]
    fn import_folder_imports_supported_files() {
        let repo = Arc::new(FakeRepo::default());
        let service = LibraryService::new(repo.clone(), Arc::new(FakeProbe));
        let dir = std::env::temp_dir().join("audiomaster-test-folder");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.mp3"), b"x").unwrap();
        std::fs::write(dir.join("b.txt"), b"y").unwrap();
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("c.wav"), b"z").unwrap();

        let summary = service.import_folder(&dir, 1, true).unwrap();
        assert_eq!(summary.imported, 2);
        assert_eq!(summary.skipped, 0);
        assert_eq!(summary.failed.len(), 0);
        assert_eq!(repo.assets.lock().unwrap().len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_folder_rejects_missing_dir() {
        let repo = Arc::new(FakeRepo::default());
        let service = LibraryService::new(repo, Arc::new(FakeProbe));
        let result = service.import_folder(std::path::Path::new("C:/no/such/dir"), 1, true);
        assert!(result.is_err());
    }

    #[test]
    fn library_query_sanitized_clamps_limit() {
        let q = LibraryQuery {
            text: "  hi  ".into(),
            limit: 0,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(q.text, "hi");
        assert_eq!(q.limit, 1);
        let q2 = LibraryQuery {
            text: "x".into(),
            limit: 500,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(q2.limit, 200);
    }
}
