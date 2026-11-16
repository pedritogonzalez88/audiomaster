He diseñado las historias de usuario con criterios de aceptación, el esquema SQL en SQLite y he generado el prototipo interactivo de emisión en Canvas.

---

### 1. Profundización Técnica e Investigación de Requerimientos

Para llevar la arquitectura definida a un estándar de nivel broadcast comercial (equivalente o superior a Dinesat 13 y RadioBoss 7.2), se integran los siguientes estándares de la industria radiofónica y de telecomunicaciones:

* **Normalización de sonoridad ([EBU R128](https://tech.ebu.ch/loudness) / [ITU-R BS.1770-4](https://www.itu.int/rec/R-REC-BS.1770)):**
* *Objetivo:* Eliminar saltos bruscos de volumen entre temas musicales, jingles y spots publicitarios.
* *Métricas en tiempo real:* Loudness integrado objetivo a **-23.0 LUFS** (tolerancia ±0.5 LU), True Peak máximo a **-1.0 dBTP**, y control de rango de sonoridad (*LRA* < 14 LU para radio FM/streaming).


* **Puntos de Cue y Matemática de Mezcla (*Crossfade*):**
* *Marcas operativas:* `Cue In` (inicio audible eliminando silencio inicial), `Intro` (segundos instrumentales antes de la voz), `Hook` (fragmento representativo para ráfagas), `Next/Mix` (punto donde arranca la siguiente pista), `Fade Out` (comienzo de atenuación) y `Outro/Cue Out` (fin de pista).
* *Curvas de atenuación:* Aplicación de curva de **potencia constante** (*Equal Power Crossfade*: $gain_{out} = \cos(t \cdot \frac{\pi}{2})$, $gain_{in} = \sin(t \cdot \frac{\pi}{2})$) para evitar la caída de energía acústica (-3 dB) que produce el fundido lineal tradicional.


* **Conmutación Satelital y Redundancia (DTMF / [SCTE-35](https://www.scte.org/standards/)):**
* Recepción de tonos DTMF en la entrada auxiliar (frecuencias estándar ITU-T Q.23) para disparo automático de desconexión local y emisión de tandas comerciales zonales.


* **Protocolos de Emisión y Metadatos:**
* *Streaming:* Inyección de cabeceras `icy-metadata` en bloques regulares de bytes a servidores Shoutcast v2 e Icecast 2.4+.
* *RDS (Radio Data System):* Emisión de tramas [UECP](https://www.google.com/search?q=https://www.rds.org.uk/standards/uecp/) (Universal Encoder Communication Protocol) o comandos serie/TCP ASCII hacia codificadores RDS para visualización dinámica de `PS` (Nombre de emisora) y `RT` (RadioTexto: Artista - Título).


* **Persistencia 24/7 en SQLite:**
* Uso estricto de `PRAGMA journal_mode = WAL` para permitir que el hilo de emisión inserte registros de auditoría (*playout logs*) concurrentemente sin bloquear las consultas de lectura de la playlist ni la búsqueda instantánea en biblioteca.



---

### 2. Historias de Usuario y Criterios de Aceptación

#### Épica 1: Motor de Playout, Reproducción y Transiciones (RF-01 a RF-09)

* **HU-01: Reproducción continua con solapamiento inteligente (*Crossfade*)**
* **Como** operador de emisión / programador musical,
* **Quiero** que el reproductor principal enlace las pistas automáticamente al alcanzar la marca de *Next*,
* **Para** mantener un flujo de transmisión continuo y profesional sin silencios ni saturación sonora.
* **Criterios de Aceptación:**
* **Dado** que una pista en aire alcanza su marca de tiempo `mix_point` (o detección automática a -14 dBFS),
* **Cuando** el motor active la pista siguiente,
* **Entonces** ambas pistas deben sonar simultáneamente aplicando una curva de potencia constante durante el tiempo de solapamiento estipulado (por defecto 3.5 segundos).
* **Dado** que el operador activa el botón `Stop al final` (`S`),
* **Cuando** la pista actual finalice,
* **Entonces** el motor detendrá el playout en pausa armada sin iniciar la siguiente pista.




* **HU-02: Instant Play / Cart Wall de disparo inmediato**
* **Como** locutor u operador en vivo,
* **Quiero** disparar jingles, cortinas y efectos sonoros mediante botones dedicados o atajos (`F1` a `F12`),
* **Para** ambientar el programa en vivo con respuesta instantánea.
* **Criterios de Aceptación:**
* **Dado** que el operador pulsa una tecla de función (`F1`–`F8`) o hace clic en un pad,
* **Cuando** el cart tenga un archivo de audio asignado,
* **Entonces** el audio debe comenzar a reproducirse con una latencia inferior a 25 milisegundos sin interrumpir el reproductor principal.
* **Dado** un pad configurado en modo `Loop`,
* **Cuando** termine su reproducción,
* **Entonces** debe reiniciarse inmediatamente sin huecos audibles hasta que se presione nuevamente para detenerlo.





#### Épica 2: Biblioteca de Medios, Indexación y Metadatos (RF-10 a RF-15)

* **HU-03: Búsqueda instantánea y filtrado avanzado**
* **Como** musicalizador de la emisora,
* **Quiero** buscar canciones por artista, título, BPM, década o etiquetas en tiempo real,
* **Para** responder inmediatamente a pedidos de oyentes o cambios en la pauta.
* **Criterios de Aceptación:**
* **Dado** un catálogo de más de 50.000 pistas indexadas en SQLite,
* **Cuando** el usuario escribe cualquier término en la barra de búsqueda,
* **Entonces** la tabla virtual FTS5 debe retornar los primeros 50 resultados en menos de 15 milisegundos.
* **Dado** un archivo de audio proveniente de WhatsApp o carpeta vigilada (*hot folder*),
* **Cuando** ingrese al sistema,
* **Entonces** debe catalogarse automáticamente en la categoría correspondiente y calcular su duración y picos de nivel.





#### Épica 3: Programación (Scheduler), Rotaciones y Tandas Comerciales (RF-16 a RF-24)

* **HU-04: Inserción y emisión estricta de tandas comerciales**
* **Como** jefe de tráfico y publicidad,
* **Quiero** programar bloques de anuncios por fecha, hora y días de la semana con control de vigencia,
* **Para** cumplir los contratos comerciales y garantizar la pauta de los anunciantes.
* **Criterios de Aceptación:**
* **Dado** un bloque comercial configurado para las `14:45:00`,
* **Cuando** se alcance la hora programada en modo automático,
* **Entonces** el motor esperará el fin de la canción en curso (o forzará la entrada según la política del evento) y reproducirá secuencialmente los spots activos.
* **Dado** un spot publicitario cuya fecha de vencimiento haya expirado,
* **Cuando** el scheduler compile la tanda,
* **Entonces** el spot no debe incluirse y debe registrarse una advertencia en el log de auditoría.





#### Épica 4: Consola Virtual, DSP y TalkOver (RF-25 a RF-32)

* **HU-05: Ducking automático / Función TalkOver**
* **Como** locutor en cabina,
* **Quiero** atenuar la música de fondo con un solo botón (`T`) o al abrir el micrófono,
* **Para** hablar con claridad sobre la música sin saturar la mezcla.
* **Criterios de Aceptación:**
* **Dado** que el reproductor principal está sonando a nivel nominal (0.0 dB),
* **Cuando** se active el modo `TalkOver`,
* **Entonces** el canal de música debe descender con una rampa suave de 200 ms hasta alcanzar -14 dBFS (configurable), manteniéndose allí hasta que se desactive.





---

### 3. Diseño del Esquema SQL en SQLite

Este esquema está diseñado bajo tercera forma normal (3NF), integrando índices de alta selectividad, claves foráneas estrictas, triggers para sincronización FTS5 y configuración para entornos de emisión 24/7:

```sql
-- ============================================================================
-- AUTOMATIZADOR BROADCAST: ESQUEMA DDL SQLITE (3NF + FTS5 + AUDITORÍA)
-- ============================================================================

-- Optimizaciones del motor para emisión continua 24/7 sin bloqueos
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA temp_store = MEMORY;
PRAGMA cache_size = -64000; -- 64 MB de caché en RAM

-- ----------------------------------------------------------------------------
-- 1. EMISORAS Y USUARIOS
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS stations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    dial_frequency TEXT,
    timezone TEXT NOT NULL DEFAULT 'America/Asuncion',
    icecast_mount TEXT,
    icecast_bitrate INTEGER DEFAULT 320,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('ADMIN', 'OPERATOR', 'PROGRAMMER', 'PRODUCER')),
    password_hash TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- ----------------------------------------------------------------------------
-- 2. CATÁLOGO DE AUDIOS Y CLASIFICACIÓN
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS categories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    color_hex TEXT DEFAULT '#3b82f6',
    is_commercial INTEGER NOT NULL DEFAULT 0 CHECK (is_commercial IN (0, 1))
);

CREATE TABLE IF NOT EXISTS media_assets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    category_id INTEGER NOT NULL,
    title TEXT NOT NULL,
    artist TEXT NOT NULL,
    album TEXT,
    release_year INTEGER,
    bpm INTEGER DEFAULT 0,
    kind TEXT NOT NULL DEFAULT 'AUDIO' CHECK (kind IN ('AUDIO', 'VIDEO')),
    file_path TEXT NOT NULL UNIQUE,
    checksum_sha256 TEXT NOT NULL,
    duration_ms INTEGER NOT NULL CHECK (duration_ms > 0),
    gain_db REAL DEFAULT 0.0,
    loudness_lufs REAL DEFAULT -23.0,
    cue_in_ms INTEGER NOT NULL DEFAULT 0,
    intro_ms INTEGER NOT NULL DEFAULT 0,
    mix_point_ms INTEGER NOT NULL DEFAULT 0,
    fade_out_ms INTEGER NOT NULL DEFAULT 0,
    cue_out_ms INTEGER NOT NULL DEFAULT 0,
    play_count INTEGER NOT NULL DEFAULT 0,
    last_played_at DATETIME,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (category_id) REFERENCES categories (id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS asset_tags (
    asset_id INTEGER NOT NULL,
    tag_id INTEGER NOT NULL,
    PRIMARY KEY (asset_id, tag_id),
    FOREIGN KEY (asset_id) REFERENCES media_assets (id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags (id) ON DELETE CASCADE
);

-- ----------------------------------------------------------------------------
-- 3. MOTOR FTS5 PARA BÚSQUEDA INSTANTÁNEA (< 5ms)
-- ----------------------------------------------------------------------------
CREATE VIRTUAL TABLE IF NOT EXISTS media_assets_fts USING fts5(
    title,
    artist,
    album,
    content='media_assets',
    content_rowid='id'
);

-- Triggers de sincronización automática FTS5
CREATE TRIGGER IF NOT EXISTS trg_media_assets_ai AFTER INSERT ON media_assets BEGIN
    INSERT INTO media_assets_fts(rowid, title, artist, album)
    VALUES (new.id, new.title, new.artist, new.album);
END;

CREATE TRIGGER IF NOT EXISTS trg_media_assets_ad AFTER DELETE ON media_assets BEGIN
    INSERT INTO media_assets_fts(media_assets_fts, rowid, title, artist, album)
    VALUES ('delete', old.id, old.title, old.artist, old.album);
END;

CREATE TRIGGER IF NOT EXISTS trg_media_assets_au AFTER UPDATE ON media_assets BEGIN
    INSERT INTO media_assets_fts(media_assets_fts, rowid, title, artist, album)
    VALUES ('delete', old.id, old.title, old.artist, old.album);
    INSERT INTO media_assets_fts(rowid, title, artist, album)
    VALUES (new.id, new.title, new.artist, new.album);
END;

-- ----------------------------------------------------------------------------
-- 4. PLAYLISTS Y COLA DE EMISIÓN (PLAYOUT QUEUE)
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS playlists (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    scheduled_for DATE,
    total_duration_ms INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS playlist_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    playlist_id INTEGER NOT NULL,
    asset_id INTEGER NOT NULL,
    position INTEGER NOT NULL,
    transition_type TEXT NOT NULL DEFAULT 'CROSSFADE' CHECK (transition_type IN ('CROSSFADE', 'CUT', 'FADE_IN')),
    crossfade_duration_ms INTEGER DEFAULT 3500,
    status TEXT NOT NULL DEFAULT 'QUEUED' CHECK (status IN ('QUEUED', 'PLAYING', 'PLAYED', 'SKIPPED')),
    scheduled_air_time TEXT,
    FOREIGN KEY (playlist_id) REFERENCES playlists (id) ON DELETE CASCADE,
    FOREIGN KEY (asset_id) REFERENCES media_assets (id) ON DELETE RESTRICT
);

-- ----------------------------------------------------------------------------
-- 5. CART WALL / INSTANT PLAY (IPLAY)
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS cartwall_pages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id INTEGER NOT NULL,
    page_number INTEGER NOT NULL CHECK (page_number BETWEEN 1 AND 10),
    name TEXT NOT NULL,
    UNIQUE (station_id, page_number),
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS cartwall_slots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    page_id INTEGER NOT NULL,
    slot_index INTEGER NOT NULL CHECK (slot_index BETWEEN 1 AND 16),
    hotkey TEXT NOT NULL,
    asset_id INTEGER,
    color_hex TEXT DEFAULT '#8b5cf6',
    is_loop INTEGER NOT NULL DEFAULT 0 CHECK (is_loop IN (0, 1)),
    gain_db REAL DEFAULT 0.0,
    UNIQUE (page_id, slot_index),
    FOREIGN KEY (page_id) REFERENCES cartwall_pages (id) ON DELETE CASCADE,
    FOREIGN KEY (asset_id) REFERENCES media_assets (id) ON DELETE SET NULL
);

-- ----------------------------------------------------------------------------
-- 6. PUBLICIDAD, TANDAS Y PAUTADO COMERCIAL
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ad_campaigns (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    advertiser_name TEXT NOT NULL,
    contract_number TEXT,
    start_date DATE NOT NULL,
    end_date DATE NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1))
);

CREATE TABLE IF NOT EXISTS ad_spots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    campaign_id INTEGER NOT NULL,
    asset_id INTEGER NOT NULL,
    priority INTEGER DEFAULT 1,
    hourly_limit INTEGER DEFAULT 2,
    FOREIGN KEY (campaign_id) REFERENCES ad_campaigns (id) ON DELETE CASCADE,
    FOREIGN KEY (asset_id) REFERENCES media_assets (id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS ad_schedules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    spot_id INTEGER NOT NULL,
    station_id INTEGER NOT NULL,
    target_time TEXT NOT NULL, -- Ej: '14:45:00'
    days_of_week TEXT NOT NULL DEFAULT '1,2,3,4,5,6,7', -- Lunes a Domingo
    FOREIGN KEY (spot_id) REFERENCES ad_spots (id) ON DELETE CASCADE,
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
);

-- ----------------------------------------------------------------------------
-- 7. AUDITORÍA DE AIRE Y TRAZABILIDAD (AIR LOGS)
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS playout_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id INTEGER NOT NULL,
    asset_id INTEGER,
    raw_title TEXT NOT NULL,
    raw_artist TEXT NOT NULL,
    category_code TEXT NOT NULL,
    origin TEXT NOT NULL CHECK (origin IN ('AUTO', 'MANUAL', 'CARTWALL', 'SCHEDULE', 'RELAY_DTMF')),
    played_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    duration_played_ms INTEGER NOT NULL,
    was_interrupted INTEGER NOT NULL DEFAULT 0 CHECK (was_interrupted IN (0, 1)),
    operator_username TEXT,
    FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE,
    FOREIGN KEY (asset_id) REFERENCES media_assets (id) ON DELETE SET NULL
);

-- ----------------------------------------------------------------------------
-- 8. ÍNDICES ESTRATÉGICOS PARA CONSULTAS < 50ms
-- ----------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_media_assets_cat_artist ON media_assets (category_id, artist);
CREATE INDEX IF NOT EXISTS idx_media_assets_checksum ON media_assets (checksum_sha256);
CREATE INDEX IF NOT EXISTS idx_playlist_items_seq ON playlist_items (playlist_id, position);
CREATE INDEX IF NOT EXISTS idx_playout_logs_query ON playout_logs (station_id, played_at);
CREATE INDEX IF NOT EXISTS idx_ad_schedules_time ON ad_schedules (station_id, target_time);

```

---

### 4. Prototipo de la Pantalla Principal de Emisión

El prototipo interactivo ha sido generado y cargado en el panel Canvas lateral (`broadcast_playout_ui.html`).

#### Aspectos clave implementados en el prototipo:

1. **Telemetría y barra superior:** Tally activo `ON AIR`, selector de modo `AUTOMÁTICO / ASISTIDO`, reloj digital maestro con sincronización de segundos en tiempo real e indicadores de streaming Icecast y grabación continua de auditoría (Logger).
2. **Módulo central de emisión (Now Playing Deck):** Waveform dinámica interactiva con marcas visibles de `Intro` y `Next Mix`, contadores en cuenta regresiva (`-02:18`), tiempo transcurrido y tiempo de intro para locución, acompañado de los controles de playout (`Espacio` para Play/Pause, `F9` para Mezcla anticipada, `S` para Stop al fin de pista y `T` para TalkOver con atenuación a -14 dB).
3. **Biblioteca y búsqueda rápida (FTS5):** Búsqueda instantánea con filtrado por categorías (Música, Jingles, Publicidad, WhatsApp) y botón de inserción directa a la cola de emisión.
4. **Cart Wall (Instant Play):** Grilla interactiva de pads con páginas `P1`, `P2`, `P3`, atajos de teclado `F1`–`F8` y retroalimentación sonora sintetizada mediante la Web Audio API (sweepers, bass drops, stingers) sin dependencias externas.
5. **Consola virtual y medidores de audio:** Vúmetros estéreo duales (*Peak VU*) con retención de picos, lectura EBU R128 (-23.1 LUFS) y faders de mezcla para Música, Carts, Micrófono y Master Out.