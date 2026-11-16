//! Spike C: seeds a synthetic library and measures the search latency of the
//! production FTS5 path, then compares it with a trigram index for substring
//! lookups.
//!
//! Run with:
//!
//! ```text
//! cargo run --release -p infra-db --example spike_c -- [rows]
//! ```
//!
//! Default is 50.000 rows, the figure used by the plan. The database lives in a
//! temporary file (not in memory) so the numbers include WAL reads and the file
//! size of both indexes is meaningful.

#![allow(clippy::print_stdout)]

fn main() {
    let rows: usize = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(50_000);

    let path = std::env::temp_dir().join(format!("audiomaster-spike-c-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);

    println!("spike C — library search over {rows} rows");

    let database = infra_db::Database::open(&path).expect("open database");
    let seed_started = std::time::Instant::now();
    seed(&database, rows);
    let seed_elapsed = seed_started.elapsed();
    println!(
        "seed    {rows} rows in {:.0} ms ({:.0} rows/s)",
        seed_elapsed.as_secs_f64() * 1000.0,
        rows as f64 / seed_elapsed.as_secs_f64()
    );
    database.analyze().expect("analyze");
    drop(database);
    print_plans(&path);

    for round in 1..=3 {
        let (broad, selective, list_latencies) = measure_queries();
        println!(
            "round {round}  broad p50 {:>5.2} ms p95 {:>5.2} | selective p50 {:>5.2} ms p95 {:>5.2} | list p50 {:>5.2} ms p95 {:>5.2}",
            percentile(&broad, 0.50),
            percentile(&broad, 0.95),
            percentile(&selective, 0.50),
            percentile(&selective, 0.95),
            percentile(&list_latencies, 0.50),
            percentile(&list_latencies, 0.95),
        );
    }

    let before_trigram = file_size(&path);
    match rank_cost(&path) {
        Ok((ranked, unranked)) => println!(
            "rank    bm25 p50 {:>5.2} ms p95 {:>5.2} | alphabetical p50 {:>5.2} ms p95 {:>5.2}",
            percentile(&ranked, 0.50),
            percentile(&ranked, 0.95),
            percentile(&unranked, 0.50),
            percentile(&unranked, 0.95),
        ),
        Err(error) => println!("rank    unavailable: {error}"),
    }

    match trigram(&path) {
        Ok(latencies) => {
            let after_trigram = file_size(&path);
            println!(
                "trigram p50 {:.2} ms  p95 {:.2} ms  max {:.2} ms  (substring, n={})",
                percentile(&latencies, 0.50),
                percentile(&latencies, 0.95),
                latencies.last().copied().unwrap_or(0.0),
                latencies.len()
            );
            println!(
                "size    {:.1} MB before trigram, {:.1} MB after ({:+.1} MB)",
                before_trigram,
                after_trigram,
                after_trigram - before_trigram
            );
        }
        Err(error) => println!("trigram unavailable: {error}"),
    }

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

const ADJECTIVES: [&str; 12] = [
    "Midnight", "Golden", "Electric", "Broken", "Silent", "Neon", "Velvet", "Paper", "Crystal",
    "Lonely", "Burning", "Endless",
];
const NOUNS: [&str; 12] = [
    "Highway", "River", "Dream", "Echo", "Garden", "Station", "Ocean", "Mirror", "Summer",
    "Shadow", "Island", "Fire",
];

fn seed(database: &infra_db::Database, rows: usize) {
    use application::{MediaRepository, ProbedMedia};
    use domain::{AssetKind, CuePoints};
    use std::fmt::Write as _;

    let mut checksum = String::with_capacity(64);
    for index in 0..rows {
        let adjective = ADJECTIVES[index % ADJECTIVES.len()];
        let noun = NOUNS[(index / ADJECTIVES.len()) % NOUNS.len()];
        let word = NOUNS[(index / 37) % NOUNS.len()];
        let title = format!("{adjective} {noun} {index:05}");
        let artist = format!("Artist {} {word}", index % 500);
        checksum.clear();
        let _ = write!(checksum, "{index:064x}");

        let probed = ProbedMedia {
            title,
            artist,
            album: Some(format!("Album {}", index % 800)),
            release_year: Some(1980 + (index % 45) as i32),
            bpm: 80 + (index % 80) as u32,
            kind: AssetKind::Audio,
            duration_ms: 180_000,
            checksum_sha256: checksum.clone(),
            loudness_lufs: -23.0,
            cue_points: CuePoints::whole_file(180_000),
        };
        let category = 1 + (index % 6) as i64;
        let file = std::path::PathBuf::from(format!("C:/media/{index:05}.mp3"));
        database
            .insert(category, &probed, &file)
            .expect("insert asset");
    }
}

fn measure_queries() -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    use application::{LibraryQuery, MediaRepository};

    let database = open_existing();
    let mut broad = Vec::new();
    let mut selective = Vec::new();
    let mut list = Vec::new();
    for pass in 0..2 {
        // Broad: one common word, the pathological case for bm25 ranking.
        for index in 0..300 {
            let query = LibraryQuery {
                text: NOUNS[index % NOUNS.len()].to_string(),
                category_id: if index % 2 == 0 { None } else { Some(1) },
                limit: 100,
                offset: 0,
            };
            let started = std::time::Instant::now();
            let results = database.search(&query).expect("search");
            if pass == 1 {
                broad.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            std::hint::black_box(results.len());
        }

        // Selective: a two-word phrase, what an operator types to find a track.
        for index in 0..300 {
            let text = format!(
                "{} {}",
                ADJECTIVES[index % ADJECTIVES.len()],
                NOUNS[(index / ADJECTIVES.len()) % NOUNS.len()]
            );
            let query = LibraryQuery {
                text,
                category_id: None,
                limit: 100,
                offset: 0,
            };
            let started = std::time::Instant::now();
            let results = database.search(&query).expect("search");
            if pass == 1 {
                selective.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            std::hint::black_box(results.len());
        }

        for _ in 0..200 {
            let query = LibraryQuery {
                text: String::new(),
                category_id: None,
                limit: 100,
                offset: 0,
            };
            let started = std::time::Instant::now();
            let results = database.search(&query).expect("search");
            if pass == 1 {
                list.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            std::hint::black_box(results.len());
        }
    }
    for latencies in [&mut broad, &mut selective, &mut list] {
        latencies.sort_by(|a, b| a.partial_cmp(b).expect("finite latency"));
    }
    (broad, selective, list)
}

fn print_plans(path: &std::path::Path) {
    let Ok(connection) = rusqlite::Connection::open(path) else {
        return;
    };

    let indexes: Vec<String> = connection
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'media_assets'",
        )
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default();
    println!("indexes {}", indexes.join(", "));

    let queries = [
        (
            "list",
            "SELECT id FROM media_assets m
             WHERE m.deleted_at IS NULL AND m.is_active = 1
             ORDER BY m.artist COLLATE NOCASE, m.title COLLATE NOCASE LIMIT 100",
        ),
        (
            "list+cat",
            "SELECT id FROM media_assets m
             WHERE m.deleted_at IS NULL AND m.is_active = 1 AND m.category_id = 1
             ORDER BY m.artist COLLATE NOCASE, m.title COLLATE NOCASE LIMIT 100",
        ),
    ];
    for (label, sql) in queries {
        let Ok(mut statement) = connection.prepare(&format!("EXPLAIN QUERY PLAN {sql}")) else {
            continue;
        };
        let Ok(rows) = statement.query_map([], |row| row.get::<_, String>(3)) else {
            continue;
        };
        let steps: Vec<String> = rows.filter_map(Result::ok).collect();
        println!("plan    {label}: {}", steps.join(" | "));
    }
}

fn rank_cost(path: &std::path::Path) -> Result<(Vec<f64>, Vec<f64>), rusqlite::Error> {
    let connection = rusqlite::Connection::open(path)?;
    let query = "Midnight* Highway*";
    let ranked_sql = "SELECT m.id FROM media_assets m
         JOIN media_assets_fts f ON f.rowid = m.id
         WHERE media_assets_fts MATCH ?1
           AND m.deleted_at IS NULL AND m.is_active = 1
         ORDER BY bm25(media_assets_fts), m.artist COLLATE NOCASE LIMIT 100";
    let unranked_sql = "SELECT m.id FROM media_assets m
         JOIN media_assets_fts f ON f.rowid = m.id
         WHERE media_assets_fts MATCH ?1
           AND m.deleted_at IS NULL AND m.is_active = 1
         ORDER BY m.artist COLLATE NOCASE, m.title COLLATE NOCASE LIMIT 100";

    let mut ranked = Vec::new();
    let mut unranked = Vec::new();
    for pass in 0..2 {
        for (sql, target) in [(ranked_sql, &mut ranked), (unranked_sql, &mut unranked)] {
            let mut statement = connection.prepare_cached(sql)?;
            for _ in 0..200 {
                let started = std::time::Instant::now();
                let rows = statement.query_map([query], |row| row.get::<_, i64>(0))?;
                let count = rows.count();
                if pass == 1 {
                    target.push(started.elapsed().as_secs_f64() * 1000.0);
                }
                std::hint::black_box(count);
            }
        }
    }
    ranked.sort_by(|a, b| a.partial_cmp(b).expect("finite latency"));
    unranked.sort_by(|a, b| a.partial_cmp(b).expect("finite latency"));
    Ok((ranked, unranked))
}

fn trigram(path: &std::path::Path) -> Result<Vec<f64>, rusqlite::Error> {
    let connection = rusqlite::Connection::open(path)?;
    connection.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS media_assets_trigram USING fts5(
             title,
             artist,
             content='media_assets',
             content_rowid='id',
             tokenize='trigram'
         );
         INSERT INTO media_assets_trigram(media_assets_trigram) VALUES('rebuild');",
    )?;

    let mut latencies = Vec::new();
    for index in 0..200 {
        let needle = match index % 5 {
            0 => "ight",
            1 => "unse",
            2 => "tist",
            3 => "eam",
            _ => "High",
        };
        let started = std::time::Instant::now();
        let mut statement = connection.prepare_cached(
            "SELECT rowid FROM media_assets_trigram WHERE title MATCH ?1 LIMIT 100",
        )?;
        let found = statement.query_map([needle], |row| row.get::<_, i64>(0))?;
        let count = found.count();
        latencies.push(started.elapsed().as_secs_f64() * 1000.0);
        std::hint::black_box(count);
    }
    latencies.sort_by(|a, b| a.partial_cmp(b).expect("finite latency"));
    Ok(latencies)
}

fn open_existing() -> infra_db::Database {
    let path = std::env::temp_dir().join(format!("audiomaster-spike-c-{}.db", std::process::id()));
    infra_db::Database::open(path).expect("reopen database")
}

fn file_size(path: &std::path::Path) -> f64 {
    let mut total = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0) as f64;
    for suffix in ["-wal", "-shm"] {
        let companion = std::path::PathBuf::from(format!("{}{suffix}", path.display()));
        total += std::fs::metadata(companion)
            .map(|meta| meta.len())
            .unwrap_or(0) as f64;
    }
    total / (1024.0 * 1024.0)
}

fn percentile(sorted: &[f64], quantile: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() as f64 - 1.0) * quantile).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}
