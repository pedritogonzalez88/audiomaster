# Plan de Implementación — Audiomaster

**Producto:** Automatizador y reproductor multimedia para broadcast, streaming y producción.
**Documentos base:** `docs/Especificaciones Reproductor Audio y Video Multiplataforma.md` (v1.0) y `docs/especificaciones_extra.md`.
**Prototipo de referencia:** `automatizador_broadcast_on_air_master_studio_playout.html`.
**Versión del plan:** 1.0 — 15/11/2026.
**Estado:** propuesto para revisión y aprobación.

---

## 0. Resumen ejecutivo

Las especificaciones cubren con solidez el **qué** (41 requisitos funcionales, 15 no funcionales, 5 épicas con criterios de aceptación, esquema SQL y un prototipo visual de alta fidelidad). La arquitectura propuesta —Tauri 2 + Rust, libmpv para video, CPAL/Symphonia para audio, SQLite/FTS5— es adecuada para el dominio, pero **todavía no es directamente implementable**: faltan piezas de modelo de datos para scheduler, rotaciones, tandas y trazabilidad; hay contradicciones entre objetivos (tamaño del instalador vs. libmpv/FFmpeg); y el motor de playout en tiempo real necesita decisiones explícitas de concurrencia, reloj y recuperación.

Este plan convierte las especificaciones en un **programa de entrega por fases**, con:

1. Un **diagnóstico** de brechas, contradicciones y decisiones pendientes (secciones 1 y 3).
2. Una **arquitectura de implementación** concreta: crates, hilos, cadena de audio, IPC, persistencia y seguridad (sección 2).
3. Un **modelo de datos corregido y ampliado** para soportar los 41 RF (sección 3).
4. Un **WBS por fases** con tareas, entregables, criterios de salida y esfuerzo (sección 4).
5. **Calidad, CI/CD, riesgos y gobernanza** (secciones 6-9).
6. Una **hoja de ruta de 10 días** para arrancar con validaciones técnicas tempranas (sección 10).

**Recomendación central:** construir primero un *vertical slice* verificable (importar → biblioteca → cola → reproducir → medir → auditar) y resolver en la Fase 0 los tres *spikes* de mayor riesgo: motor de audio en tiempo real, integración de libmpv en Tauri y rendimiento de FTS5 a 50.000 pistas. No avanzar a automatización hasta que ese slice cumpla latencia, underruns y recuperación.

**Alcance propuesto para v1.0:** Fases 0 a 5 (playout, biblioteca, automatización, broadcast, red). Video/radio visual, IA de voz, VST y SCTE-35 quedan para v1.1/v2 con *feature flags* y ADRs propios.

---

## 1. Diagnóstico de las especificaciones

### 1.1 Cobertura

| Área | Estado | Observación |
|---|---|---|
| Visión y alcance | Completo | Benchmark Dinesat/RadioBoss bien documentado y trazable. |
| Requisitos funcionales | Completo (41 RF) | Falta criterio de aceptación para ~26 RF (solo 5 HU cubren 9 RF). |
| Requisitos no funcionales | Completo (15 RNF) | Algunas metas son incompatibles entre sí (ver G-01, G-02). |
| UX/UI | Completo conceptual + prototipo | El prototipo es un contrato visual, no código de producción. |
| Arquitectura | Sólida | Clean/hexagonal correcta; faltan detalles de tiempo real, video y sync. |
| Modelo de datos | Parcial | Cubre biblioteca, cola, cart wall y auditoría; faltan scheduler, rotación, tráfico y operación. |
| Pruebas | Estratégica | Falta infraestructura de audio virtual y corpus reproducible. |
| Operación | Ausente | No hay plan de instalación, actualización, backups, soporte ni *runbooks*. |

### 1.2 Brechas, contradicciones y riesgos de especificación

| ID | Severidad | Hallazgo | Impacto | Acción propuesta |
|---|---|---|---|---|
| G-01 | Alta | **RNF-05 (<25 MB instalado)** es incompatible con empaquetar libmpv + FFmpeg + WebView. Un build típico de libmpv/FFmpeg pesa 40-80 MB. | Reescritura de arquitectura a mitad de proyecto o incumplimiento de RNF. | Revisar meta a **≤80 MB** (instalador con libmpv) o **≤30 MB** con libmpv del sistema/descarga opcional. ADR-007. |
| G-02 | Alta | **RNF-03 (RAM ≤150 MB)** con video activo/libmpv y WebView no está garantizado. | Expectativa incorrecta en acceptance. | Definir presupuesto separado: reposo audio ≤150 MB; video ≤300 MB; medir con `dhat`/`heaptrack`. |
| G-03 | Alta | **RNF-08 (CRDT, 10+ terminales)** choca con SQLite de escritor único. | Diseño de sync inviable si se toma literal. | v1.0: servidor de escritura única + API REST y concurrencia optimista; CRDT solo para edición de programación en v2. ADR-010. |
| G-04 | Alta | **HU-02 exige <25 ms** para cart wall, mientras **RNF-02** permite <50 ms. | Criterio no medible de forma uniforme. | Unificar: **P95 ≤30 ms** en Windows/macOS con buffer 256 muestras; P95 ≤50 ms en Linux. Medir de tecla a primer sample. |
| G-05 | Alta | El esquema **no tiene scheduler** (RF-16): no existen `schedule_events`, recurrencias, acciones ni condiciones DTMF. | RF-16 a RF-24 no implementables sin rediseño. | Añadir tablas de la sección 4.2. |
| G-06 | Alta | El esquema **no tiene motor de rotación** (RF-17, RF-19): faltan filtros, cuotas, separación y dayparts. | El generador de playlists sería código ad hoc no auditable. | Añadir `rotation_rules`, `rotation_terms`, `separation_rules`, `dayparts`. |
| G-07 | Alta | `playlist_items.asset_id NOT NULL` impide streams, comandos, hora/clima, retransmisiones (RF-20, RF-23). | Modelo cerrado a un solo tipo de ítem. | Añadir `item_type` y `asset_id` nullable con `CHECK`. |
| G-08 | Media | `ad_schedules` no tiene vigencia propia, prioridad, peso ni topes; `hourly_limit` no se aplica. | Riesgo de incumplimiento contractual y doble emisión. | Añadir `ad_airings`, `priority`, `weight`, topes por hora/día y validación en compilación de tanda. |
| G-09 | Media | **FTS5 externa** sin `busy_timeout`, sin tokenizer declarado y sin estrategia para tags. | Búsqueda lenta o desincronizada tras import masivo. | Definir tokenizer `unicode61 remove_diacritics 2` + índice trigram; triggers sobre `asset_tags`; comando `rebuild`. |
| G-10 | Media | `playlist_items.status` mezcla estado persistente con estado transitorio (`PLAYING`). | Estados inconsistentes tras caída. | Persistir solo `PENDING/PLAYED/SKIPPED`; estado de motor en memoria + snapshot. |
| G-11 | Media | Faltan campos de cue: `hook`, `outro`, `fade_in`, `true_peak`, `LRA`; `mix_point_ms` único es insuficiente. | Crossfade fino y ráfagas no implementables. | Ampliar `media_assets` (sección 4.1). |
| G-12 | Media | **Sin modelo de video**: no hay salida, ventana, sincronía A/V ni modo radio visual. | RF-01/02/03 y fase 4 quedan ambiguos. | ADR-003: superficie de video, audio de video y reloj maestro. |
| G-13 | Media | **Secretos de streaming** no modelados (host, puerto, TLS, password); `stations` solo tiene mount/bitrate. | Credenciales en texto plano o fuera del sistema. | `stream_targets` + `keyring` (DPAPI/Keychain/Secret Service). |
| G-14 | Media | **Sin RBAC granular ni sesiones**; `users.role` no alcanza para acceso por emisora. | RNF-09 incompleto. | `user_station_roles`, `sessions`, `audit_actions`; Argon2id. |
| G-15 | Media | `CURRENT_TIMESTAMP` en SQLite es UTC sin marca; `scheduled_air_time TEXT` es ambiguo; `stations.timezone` no se usa. | Errores de ±horas en parrilla y reportes. | Convención UTC epoch ms + conversión con `chrono-tz` en el borde. |
| G-16 | Media | No hay **migraciones versionadas**, backups ni retención. | Imposible actualizar en producción 24/7. | `schema_migrations`, `VACUUM INTO`, backup programado y *restore test*. |
| G-17 | Media | **Licencias**: libmpv es GPLv2+ por defecto; FFmpeg puede ser GPL; VST y ASIO tienen licencias propias. | Bloqueo legal de distribución. | Revisión legal + build LGPL, enlace dinámico, avisos y SBOM. ADR-007. |
| G-18 | Baja | El prototipo usa **Tailwind por CDN** y sonidos sintetizados. | No funciona offline ni representa audio real. | Reescribir como componentes React con Tailwind compilado; datos reales vía IPC. |
| G-19 | Baja | No hay criterios de accesibilidad para waveform/canvas ni atajos configurables. | RNF-11 incompleto. | Waveform con alternativa textual, `aria-live`, mapa de atajos editable. |
| G-20 | Baja | No hay estrategia de **i18n en Rust** (mensajes de error). | RNF-12 incompleto. | Errores como códigos estables; traducción en UI. |
| G-21 | Baja | No se definen **hot folders/WhatsApp** (RF-14) ni deduplicación por checksum. | Ingesta manual y duplicados. | `hot_folders`, `ingest_jobs`; checksum único lógico + cuarentena. |
| G-22 | Baja | RDS/DTMF/SCTE-35 se mencionan sin interfaz ni simulador. | Integración tardía y difícil de probar. | Puertos `RdsEncoder`, `DtmfTransport`, `RelayControl` + simuladores. |

### 1.3 Decisiones abiertas y recomendación

| ID | Decisión | Recomendación | Momento |
|---|---|---|---|
| D-01 | Framework UI | **React 19 + TypeScript + Vite** (ecosistema, accesibilidad, contratación). Svelte es alternativa válida. | Fase 0 |
| D-02 | Empaquetado de video | Ventana nativa separada de libmpv (más estable en Wayland) antes que superficie embebida. | Spike F0 |
| D-03 | Alcance de video en v1.0 | Reproducción de video en ventana dedicada + audio de video como fuente; “radio visual” completa en v1.1. | Fase 0 |
| D-04 | Sync multi-terminal | Servidor de escritura única en v1.0; CRDT/offline en v2. | Fase 5 |
| D-05 | Autenticación | Local (Argon2id) en v1.0; OIDC/LDAP en versión servidor. | Fase 5 |
| D-06 | Plugins VST/WinAMP | Fuera de v1.0. Cadena DSP propia primero; plugins en v1.2 con aislamiento de proceso. | Fase 6 |
| D-07 | IA de voz/cápsulas | Fuera de v1.0; integración por proveedor externo detrás de puerto. | Fase 6 |
| D-08 | RDS | UECP sobre TCP/serie con simulador; hardware real en *hardware-in-the-loop* opcional. | Fase 4 |
| D-09 | SCTE-35 | Solo lectura/registro en v1.1; inserción completa requiere hardware y certificación. | v1.1 |
| D-10 | Editor de audio | v1.1. En v1.0 solo cue points y procesamiento batch (normalización/BPM/silencios). | v1.1 |

### 1.4 Supuestos del plan

- Equipo base: 2 desarrolladores Rust senior, 1 desarrollador frontend senior, 1 QA/ingeniero de audio, 0,5 UX y 0,5 DevOps.
- Un solo producto/emisora activa por instancia en v1.0; el esquema y el motor se diseñan multi-emisora desde el inicio.
- No hay datos existentes que migrar: la primera migración es la línea base.
- Windows 10/11 x64, macOS 13+ (Intel/ARM), Linux x86_64/ARM64 con WebKitGTK.
- Se permite descargar componentes opcionales (libmpv/FFmpeg) en el instalador o al primer uso, según ADR-007.

---

## 2. Arquitectura de implementación

### 2.1 Stack técnico

| Capa | Tecnología | Notas |
|---|---|---|
| Shell | Tauri 2 | IPC tipado, capabilities mínimas, updater firmado. |
| Dominio/aplicación | Rust estable, `thiserror` | Sin dependencias de Tauri, SQLite ni audio. |
| Audio I/O | CPAL | Backends WASAPI/CoreAudio/ALSA; ASIO opcional y licenciado. |
| Decodificación audio | Symphonia (+ FFmpeg fallback) | 48 kHz, f32, resampling en hilo de decode. |
| DSP | Biquad/fundsp + DSP propio | EQ 12 bandas, compresor, limiter true-peak, ducking. |
| Loudness | `ebur128` / implementación propia validada | ITU-R BS.1770-4, EBU R128. |
| Video | libmpv (`libmpv2`) | `hwdec=auto`, ventana/render surface según ADR-003. |
| Persistencia | SQLite bundled + FTS5 (`rusqlite`) | WAL, escritor dedicado, migraciones propias. |
| Streaming | Cliente Shoutcast v2 / Icecast 2.4+ | HTTP/ICY, TLS, metadata `icy-metadata`. |
| Red/API | Axum (modo servidor) + reqwest | REST v1; gRPC opcional en v2. |
| UI | React 19 + TypeScript + Vite | Tailwind v4 compilado, Radix, dnd-kit, TanStack. |
| Estado UI | Zustand + TanStack Query | Estado de servidor vs. telemetría en tiempo real. |
| i18n | `react-i18next` + `chrono-tz` | es-ES, es-419, pt-BR, en-US. |
| Observabilidad | `tracing` + `tracing-appender` | Logs JSON rotativos, métricas internas. |
| Testing | `cargo-nextest`, `proptest`, `criterion`, Playwright | Sinks de audio nulos para CI. |

### 2.2 Estructura del monorepo

```
audiomaster/
├── Cargo.toml                     # workspace
├── crates/
│   ├── domain/                    # entidades, value objects, reglas puras
│   ├── application/               # casos de uso + puertos (traits)
│   ├── playout-engine/            # máquina de estados, reloj, crossfade, scheduler runtime
│   ├── infrastructure/
│   │   ├── audio/                 # CPAL, Symphonia, DSP, sinks
│   │   ├── mpv/                   # adaptador libmpv y superficie de video
│   │   ├── db/                    # SQLite, migraciones, repositorios
│   │   ├── streaming/             # Icecast/Shoutcast, metadata
│   │   ├── rds/                   # UECP / ASCII
│   │   ├── net/                   # API, sync, relay/DTMF
│   │   └── telemetry/             # tracing, métricas
│   └── tauri-app/                 # comandos, eventos, capabilities
├── ui/                            # React + design system
│   ├── src/components/
│   ├── src/features/{playout,library,schedule,traffic,reports,settings}
│   ├── src/design-system/
│   └── src/i18n/
├── tests/
│   ├── corpus/                    # archivos de audio/video licenciados para test
│   ├── integration/               # adaptadores reales + SQLite temporal
│   ├── audio-smoke/               # underruns, loudness, crossfade
│   └── e2e/                       # Playwright + tauri-driver
├── docs/
│   ├── adr/
│   ├── runbooks/
│   └── plan_implementacion.md
├── .github/workflows/
└── justfile                       # tareas reproducibles
```

Reglas de dependencia (verificables con `cargo-deny`/`cargo-machete` y revisión):

1. `domain` no depende de ningún otro crate del workspace.
2. `application` depende solo de `domain`.
3. `playout-engine` depende de `domain` y `application`; nunca de `infrastructure` ni `tauri`.
4. `infrastructure/*` implementa puertos; puede depender de `application`/`domain`.
5. `tauri-app` es el único que conoce Tauri, y compone adaptadores.
6. `ui` solo habla por comandos/eventos Tauri; nunca accede a archivos ni red directamente.

### 2.3 Modelo de concurrencia y tiempo real

```
┌──────────────────── UI (WebView) ────────────────────┐
│ React: estado derivado, telemetría 20-30 Hz, atajos  │
└───────────────▲──────────────────────┬───────────────┘
                │ eventos Tauri        │ comandos Tauri
┌───────────────┴──────────────────────▼───────────────┐
│ tauri-app: validación, DTOs, serialización, permisos │
└───────────────▲──────────────────────┬───────────────┘
                │ snapshots/eventos    │ casos de uso
┌───────────────┴──────────────────────▼───────────────┐
│ application: PlaylistService, SchedulerService, ...  │
└───────────────▲──────────────────────┬───────────────┘
                │                      │
┌───────────────┴──────────┐  ┌────────▼───────────────┐
│ playout-engine           │  │ infrastructure         │
│ hilo control 5-10 ms     │  │ DB writer, streaming,  │
│ scheduler, crossfade     │  │ RDS, ingest, telemetry │
└───────────┬──────────────┘  └────────────────────────┘
            │ comandos lock-free + PCM ring buffers
┌───────────▼──────────────────────────────────────────┐
│ audio callback (RT) — sin locks, sin allocs, sin I/O │
│ mezcla decks + carts + mic → EQ → duck → limiter     │
└──────────────────────────────────────────────────────┘
```

Hilos y responsabilidades:

| Hilo | Prioridad | Responsabilidad | Regla dura |
|---|---|---|---|
| Audio callback | RT | Leer ring buffers, mezclar, DSP, escribir al device | Cero allocaciones, cero locks bloqueantes, cero syscalls. |
| Control de playout | Alta | Tick 5-10 ms, máquina de estados, crossfade, lookahead | Publica comandos por canal acotado; nunca toca el device. |
| Decoder por deck | Normal | Symphonia/FFmpeg → resample → ring buffer | Pre-buffer 2-5 s; reporta underrun. |
| Mic input | RT | Captura y ring buffer con corrección de deriva | Si es otro device, resampling adaptativo. |
| Scheduler | Normal | Evalúa eventos cada 500 ms, materializa 30 s antes | Reloj monotónico + UTC; tolerante a saltos. |
| DB writer | Normal | Inserta `playout_logs`, `ad_airings`, snapshots | Batch + `busy_timeout`; nunca bloquea al control. |
| Streaming | Normal | Encoder + HTTP/ICY, reconexión con backoff | Buffer independiente; no afecta al aire local. |
| Telemetría | Baja | Agrega métricas y publica eventos a UI | Máx. 30 Hz; payload pequeño. |

Decisiones de implementación:

- **Reloj maestro:** contador de samples reproducidos, corregido por `Instant`; el scheduler usa `Utc::now()` y `chrono-tz`.
- **Buffers:** salida objetivo 20-50 ms; decoder 2-5 s; comandos por `crossbeam-channel` acotado; PCM por `ringbuf` SPSC.
- **Configuración:** `arc-swap` para snapshots inmutables leídos por el callback.
- **Falla segura:** si falta un archivo o falla un decode, se salta al siguiente ítem, se emite silencio/filler y se registra alerta; el motor **nunca** detiene la emisión por un error de recurso.
- **Recuperación:** snapshot de estado cada 5 s (cola, posición, tanda, modo) + journal de transiciones; al arrancar se ofrece “reanudar emisión” en <10 s.
- **Watchdog:** hilo supervisor detecta callback detenido (>500 ms sin avanzar) y reinicia el device con degradación controlada.

### 2.4 Cadena de audio

```
Decks (2) ─┐
Carts (N) ─┤
Mic ───────┼─► Gain por canal ─► EQ 12 bandas ─► Ducking ─► Compresor ─► Limiter ─► Master
Aux/Relay ─┘        │                                   ▲ true peak -1 dBTP
                    └─► Medición LUFS/TruePeak ────────┘
Master ─► Salida física (multicanal 2.0/4.0/5.1/7.1)
       ├─► Encoder streaming
       └─► Logger (grabación continua)
```

- **Formato interno:** f32, 48 kHz, hasta 8 canales; conversión y *downmix* configurables.
- **Crossfade:** curva de potencia constante (`cos/sin`), duración por transición, opción de *bass swap* y EQ de mezcla. Test: suma de potencias ≈ 1 (±0,5 dB).
- **Nivelación:** ganancia calculada al importar para -23 LUFS integrado, techo -1 dBTP; se guarda separada de la ganancia manual del operador.
- **Ducking/TalkOver:** sidechain del mic (attack 20 ms, release 300 ms, profundidad -14 dB por defecto), además del botón manual con rampa de 200 ms.
- **Limitador:** lookahead 5 ms, techo true peak configurable; nunca en el path de monitoreo de cue.
- **Salidas múltiples:** selección de device, canales, latencia y *exclusive mode* (WASAPI) por perfil.
- **Deriva de reloj:** si mic y salida usan devices distintos, resampler adaptativo con buffer de 200 ms; preferir duplex en el mismo device.

### 2.5 Video y radio visual

- **Reproducción de video:** libmpv con `hwdec=auto-safe`, ventana nativa separada o superficie embebida (ADR-003). Controles propios en la UI; no se usa el OSD de mpv.
- **Audio de video:** para emisión de radio se decodifica la pista de audio con FFmpeg/Symphonia y entra al mismo grafo DSP; el video se reproduce muteado y sincronizado por reloj maestro.
- **Radio visual:** en v1.1, una salida de video compuesta (cámara + overlays de metadata) se renderiza en ventana/proceso separado; el aire de audio no depende del video.
- **Formatos:** MP4/MKV/AVI/MOV/WebM como objetivo; la cobertura real se mide con el corpus (RNF-14).
- **Aislamiento:** un fallo de video no debe afectar el audio; el adaptador de video corre en proceso/hilo separado y se puede deshabilitar por configuración.

### 2.6 Puertos principales (traits)

```rust
// application/src/ports
pub trait AudioEnginePort { /* load, play, pause, stop, crossfade, set_gain, meters */ }
pub trait MediaRepository { /* CRUD assets, búsquedas FTS, tags */ }
pub trait PlaylistRepository { /* cola, playlists, transiciones */ }
pub trait ScheduleRepository { /* eventos, recurrencias, acciones */ }
pub trait TrafficRepository { /* campañas, spots, tandas, airings */ }
pub trait PlayoutLogPort { /* append-only, batch */ }
pub trait StreamEncoderPort { /* start, stop, send_pcm, metadata */ }
pub trait RdsEncoderPort { /* set_ps, set_rt, uecp */ }
pub trait DtmfPort { /* decode stream, emit code */ }
pub trait SecretStorePort { /* get/set/delete credenciales */ }
pub trait ClockPort { /* now_utc, monotonic, sleep_until */ }
```

Cada puerto tiene un adaptador real y uno de test (null/in-memory). Esto habilita CI sin hardware de audio.

### 2.7 Persistencia

- **Motor:** SQLite *bundled* con FTS5 y JSON1; `rusqlite` con pool de lectura y **un solo escritor** (hilo dedicado).
- **PRAGMAs:** `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout=5000`, `temp_store=MEMORY`, `cache_size=-64000`, `wal_autocheckpoint=1000`.
- **Migraciones:** tabla `schema_migrations(version, applied_at, checksum)`; runner idempotente al arranque; migraciones inmutables y probadas con base real.
- **Timestamps:** UTC epoch en milisegundos (`INTEGER`) en tablas nuevas; las existentes se migran. La zona de la emisora se aplica solo en presentación y en la compilación de parrilla.
- **Búsqueda:** FTS5 `unicode61 remove_diacritics 2` para palabras y tokenizer `trigram` para subcadenas; triggers en `media_assets` y `asset_tags`; comando `rebuild` tras import masivo. Benchmark obligatorio: 50.000 filas, P95 <15 ms.
- **Backups:** `VACUUM INTO` cada N horas + retención configurable; *restore test* automatizado semanal; nunca copiar el archivo con WAL activo.
- **Hot path de auditoría:** cola acotada al DB writer, inserción en batch de 100-500 filas o cada 500 ms; si la cola se llena, se degrada a archivo JSONL y se reconcilia.
- **Rutas de medios:** se guarda ruta canónica + checksum SHA-256 + volumen; detección de duplicados por checksum; archivos ausentes se marcan, no se borran.

### 2.8 Seguridad y secretos

- **Contraseñas:** Argon2id con parámetros versionados; bloqueo por intentos; cambio obligatorio en primer login.
- **RBAC:** roles ADMIN/OPERATOR/PROGRAMMER/PRODUCER + permisos granulares (`playout.control`, `traffic.edit`, `reports.export`, `settings.manage`) + acceso por emisora.
- **Sesiones:** token local con expiración; en modo servidor, JWT corto + refresh.
- **Secretos:** `keyring` (DPAPI/Keychain/Secret Service); nunca en SQLite ni en logs. Fallback: archivo cifrado con clave derivada del SO y permisos restrictivos.
- **Tauri:** CSP estricta, `capabilities` mínimas, sin acceso remoto al IPC, sin `shell.open` arbitrario, updater con firma y rollback.
- **Red:** TLS para streaming y API; validación de certificados; *pinning* opcional; rate limiting en modo servidor.
- **Datos:** cifrado en reposo opcional (SQLCipher) en v1.1; backups cifrados.

### 2.9 Observabilidad

- **Logs:** `tracing` JSON con `station_id`, `session_id`, `track_id`, `event_id`; rotación diaria y retención configurable.
- **Métricas:** underruns, latencia de callback, nivel de buffers, LUFS/true peak, cola de DB, reconexiones de stream, eventos saltados, tiempo de arranque.
- **Salud:** endpoint `/health` en modo servidor; panel “Diagnóstico” en UI con snapshot exportable.
- **Errores:** códigos estables (`AUDIO_DEVICE_LOST`, `ASSET_MISSING`, `DB_BUSY`, `STREAM_AUTH`) traducidos en UI; sin mensajes técnicos crudos.

---

## 3. Plan de datos: correcciones y ampliaciones

### 3.1 Correcciones al DDL existente

| # | Cambio | Motivo |
|---|---|---|
| C-01 | `playlist_items`: añadir `item_type`, permitir `asset_id NULL`, `CHECK` por tipo | Streams, comandos, hora/clima, relay. |
| C-02 | `playlist_items`: `position INTEGER` → `sort_key TEXT` (LexoRank) o `REAL` | Drag-and-drop sin reescrituras masivas. |
| C-03 | `playlist_items.status`: `QUEUED/PLAYING/...` → `PENDING/PLAYED/SKIPPED` | El estado transitorio vive en el motor. |
| C-04 | `media_assets`: añadir `hook_ms`, `outro_ms`, `fade_in_ms`, `true_peak_dbtp`, `lra_lu`, `normalization_gain_db`, `waveform_peaks BLOB`, `deleted_at`, `is_active` | Cue completo, loudness, waveform y borrado lógico. |
| C-05 | `media_assets.file_path`: normalizar y usar `COLLATE NOCASE` en Windows; unicidad lógica por checksum | Rutas duplicadas por mayúsculas/volumen. |
| C-06 | `stations`: `name COLLATE NOCASE`; separar credenciales a `stream_targets` | Consistencia y seguridad. |
| C-07 | `users`: `password_algo`, `password_updated_at`, `failed_attempts`, `locked_until`, `last_login_at` | RNF-09. |
| C-08 | `playout_logs`: añadir `playlist_item_id`, `event_id`, `ad_spot_id`, `started_at`, `ended_at`, `ended_reason`, `loudness_lufs`, `true_peak_dbtp`, `relay_source_id` | Auditoría y reportes reales. |
| C-09 | `ad_schedules`: añadir `start_date`, `end_date`, `priority`, `weight`, `max_per_hour`, `max_per_day` | Tráfico comercial. |
| C-10 | Añadir `PRAGMA busy_timeout` y `wal_autocheckpoint` | 24/7 sin `SQLITE_BUSY`. |
| C-11 | FTS5: tokenizer explícito + `rebuild` + triggers de tags | Búsqueda correcta y desincronización evitable. |
| C-12 | `cartwall_slots`: `UNIQUE(station_id, hotkey)` vía página | Evitar atajos duplicados. |

### 3.2 Tablas nuevas (DDL de referencia)

```sql
-- ============================================================================
-- 0012_scheduler_rotation_traffic.sql  (extracto)
-- ============================================================================

CREATE TABLE IF NOT EXISTS schema_migrations (
    version     INTEGER PRIMARY KEY,
    applied_at  INTEGER NOT NULL,
    checksum    TEXT NOT NULL
);

-- Scheduler de eventos (RF-16, RF-18, RF-22)
CREATE TABLE IF NOT EXISTS schedule_events (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id      INTEGER NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    trigger_kind    TEXT NOT NULL CHECK (trigger_kind IN ('AT','RECURRING','DTMF','MANUAL','API')),
    at_utc_ms       INTEGER,
    recurrence_json TEXT,          -- RRULE/JSON validado en aplicación
    dtmf_code       TEXT,
    start_date      TEXT,
    end_date        TEXT,
    daypart_id      INTEGER REFERENCES dayparts(id) ON DELETE SET NULL,
    priority        INTEGER NOT NULL DEFAULT 100,
    is_active       INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    CHECK (
        (trigger_kind = 'AT' AND at_utc_ms IS NOT NULL) OR
        (trigger_kind = 'RECURRING' AND recurrence_json IS NOT NULL) OR
        (trigger_kind = 'DTMF' AND dtmf_code IS NOT NULL) OR
        trigger_kind IN ('MANUAL','API')
    )
);

CREATE TABLE IF NOT EXISTS schedule_actions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id        INTEGER NOT NULL REFERENCES schedule_events(id) ON DELETE CASCADE,
    position        INTEGER NOT NULL,
    action_kind     TEXT NOT NULL CHECK (action_kind IN
                     ('PLAY_ASSET','PLAY_PLAYLIST','PLAY_AD_BREAK','INSERT_JINGLE',
                      'RELAY_ON','RELAY_OFF','STOP','FADE','SET_MODE','RUN_COMMAND','HTH')),
    payload_json    TEXT NOT NULL DEFAULT '{}',
    transition      TEXT NOT NULL DEFAULT 'CROSSFADE',
    UNIQUE (event_id, position)
);

-- Rotación musical (RF-17, RF-19)
CREATE TABLE IF NOT EXISTS rotation_rules (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id      INTEGER NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    daypart_id      INTEGER REFERENCES dayparts(id) ON DELETE SET NULL,
    target_slots    INTEGER NOT NULL DEFAULT 10,
    energy_profile  TEXT,
    is_active       INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1))
);

CREATE TABLE IF NOT EXISTS rotation_terms (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    rule_id         INTEGER NOT NULL REFERENCES rotation_rules(id) ON DELETE CASCADE,
    field           TEXT NOT NULL CHECK (field IN
                     ('CATEGORY','GENRE','ARTIST','DECADE','YEAR','BPM','TAG','LANGUAGE','MOOD')),
    operator        TEXT NOT NULL CHECK (operator IN ('EQ','NE','IN','BETWEEN','CONTAINS')),
    value_json      TEXT NOT NULL,
    weight          REAL NOT NULL DEFAULT 1.0,
    is_exclusion    INTEGER NOT NULL DEFAULT 0 CHECK (is_exclusion IN (0,1))
);

CREATE TABLE IF NOT EXISTS separation_rules (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id      INTEGER NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    scope           TEXT NOT NULL CHECK (scope IN ('ARTIST','TITLE','ALBUM','CATEGORY','TAG')),
    min_minutes     INTEGER NOT NULL DEFAULT 0,
    min_tracks      INTEGER NOT NULL DEFAULT 0,
    is_active       INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1))
);

CREATE TABLE IF NOT EXISTS dayparts (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id      INTEGER NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    start_minute    INTEGER NOT NULL CHECK (start_minute BETWEEN 0 AND 1439),
    end_minute      INTEGER NOT NULL CHECK (end_minute BETWEEN 0 AND 1440),
    days_of_week    TEXT NOT NULL DEFAULT '1,2,3,4,5,6,7',
    CHECK (end_minute > start_minute)
);

-- Tráfico comercial (RF-22, RF-24)
CREATE TABLE IF NOT EXISTS ad_airings (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    spot_id         INTEGER NOT NULL REFERENCES ad_spots(id) ON DELETE RESTRICT,
    station_id      INTEGER NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    scheduled_utc_ms INTEGER NOT NULL,
    started_utc_ms  INTEGER,
    ended_utc_ms    INTEGER,
    status          TEXT NOT NULL DEFAULT 'SCHEDULED'
                     CHECK (status IN ('SCHEDULED','PLAYED','SKIPPED','FAILED')),
    playout_log_id  INTEGER REFERENCES playout_logs(id) ON DELETE SET NULL,
    UNIQUE (spot_id, station_id, scheduled_utc_ms)
);

-- Streaming y credenciales (RF-33)
CREATE TABLE IF NOT EXISTS stream_targets (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id      INTEGER NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL CHECK (kind IN ('ICECAST','SHOUTCAST','HTTP')),
    host            TEXT NOT NULL,
    port            INTEGER NOT NULL,
    mount           TEXT,
    protocol        TEXT NOT NULL DEFAULT 'http' CHECK (protocol IN ('http','https')),
    bitrate_kbps    INTEGER NOT NULL DEFAULT 128,
    codec           TEXT NOT NULL DEFAULT 'MP3' CHECK (codec IN ('MP3','AAC','OPUS')),
    secret_ref      TEXT NOT NULL,   -- clave en keyring, nunca el secreto
    metadata_mode   TEXT NOT NULL DEFAULT 'ICY'
                     CHECK (metadata_mode IN ('ICY','HTTP','XML','FTP','NONE')),
    is_active       INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
    UNIQUE (station_id, kind, host, port, mount)
);

-- Logger / grabación continua (RF-31)
CREATE TABLE IF NOT EXISTS logger_recordings (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id      INTEGER NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    source          TEXT NOT NULL CHECK (source IN ('MASTER','AUX','MIC','RELAY','STREAM')),
    file_path       TEXT NOT NULL,
    started_utc_ms  INTEGER NOT NULL,
    ended_utc_ms    INTEGER,
    size_bytes      INTEGER,
    checksum_sha256 TEXT,
    retention_days  INTEGER NOT NULL DEFAULT 30
);

-- DTMF y relay (RF-23, RF-30)
CREATE TABLE IF NOT EXISTS dtmf_events (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id      INTEGER NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    direction       TEXT NOT NULL CHECK (direction IN ('RX','TX')),
    code            TEXT NOT NULL,
    source          TEXT NOT NULL,
    detected_utc_ms INTEGER NOT NULL,
    action_taken    TEXT,
    event_id        INTEGER REFERENCES schedule_events(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS relay_sources (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id      INTEGER NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    input_kind      TEXT NOT NULL CHECK (input_kind IN ('DEVICE','STREAM','SATELLITE')),
    config_json     TEXT NOT NULL,
    local_ad_replace INTEGER NOT NULL DEFAULT 0 CHECK (local_ad_replace IN (0,1)),
    is_active       INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1))
);

-- Ingesta (RF-14)
CREATE TABLE IF NOT EXISTS hot_folders (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id      INTEGER NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    path            TEXT NOT NULL,
    default_category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
    default_tags    TEXT,
    is_active       INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
    UNIQUE (station_id, path)
);

CREATE TABLE IF NOT EXISTS ingest_jobs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    hot_folder_id   INTEGER REFERENCES hot_folders(id) ON DELETE SET NULL,
    source_kind     TEXT NOT NULL CHECK (source_kind IN ('HOT_FOLDER','WHATSAPP','MANUAL','CD')),
    source_ref      TEXT,
    status          TEXT NOT NULL DEFAULT 'PENDING'
                     CHECK (status IN ('PENDING','RUNNING','DONE','FAILED','QUARANTINED')),
    asset_id        INTEGER REFERENCES media_assets(id) ON DELETE SET NULL,
    error_code      TEXT,
    created_utc_ms  INTEGER NOT NULL,
    updated_utc_ms  INTEGER NOT NULL
);

-- Acceso y auditoría (RNF-09)
CREATE TABLE IF NOT EXISTS user_station_roles (
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    station_id      INTEGER NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    role            TEXT NOT NULL CHECK (role IN ('ADMIN','OPERATOR','PROGRAMMER','PRODUCER')),
    permissions_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (user_id, station_id)
);

CREATE TABLE IF NOT EXISTS audit_actions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id         INTEGER REFERENCES users(id) ON DELETE SET NULL,
    station_id      INTEGER REFERENCES stations(id) ON DELETE SET NULL,
    action          TEXT NOT NULL,
    entity          TEXT,
    entity_id       TEXT,
    payload_json    TEXT,
    at_utc_ms       INTEGER NOT NULL
);

-- Configuración y snapshots
CREATE TABLE IF NOT EXISTS app_settings (
    key             TEXT PRIMARY KEY,
    value_json      TEXT NOT NULL,
    updated_utc_ms  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS playout_snapshots (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    station_id      INTEGER NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    at_utc_ms       INTEGER NOT NULL,
    state_json      TEXT NOT NULL,
    is_clean_shutdown INTEGER NOT NULL DEFAULT 0 CHECK (is_clean_shutdown IN (0,1))
);

-- Índices adicionales
CREATE INDEX IF NOT EXISTS idx_schedule_events_due ON schedule_events (station_id, is_active, at_utc_ms);
CREATE INDEX IF NOT EXISTS idx_ad_airings_query ON ad_airings (station_id, scheduled_utc_ms, status);
CREATE INDEX IF NOT EXISTS idx_playout_logs_time ON playout_logs (station_id, started_at);
CREATE INDEX IF NOT EXISTS idx_ingest_jobs_status ON ingest_jobs (status, created_utc_ms);
CREATE INDEX IF NOT EXISTS idx_dtmf_events_time ON dtmf_events (station_id, detected_utc_ms);
```

### 3.3 Convenciones de datos

- Identificadores internos: `INTEGER` autoincrementales; expuestos como `string` en IPC para evitar colisiones de precisión en JS.
- Fechas: `*_utc_ms INTEGER` en tablas nuevas; `DATETIME` existente se migra a epoch ms.
- Dinero/tiempo de contrato: fechas `YYYY-MM-DD` en zona de la emisora; se convierten a UTC al compilar.
- Enums: `TEXT` con `CHECK` (legible y migrable); la aplicación usa enums Rust con `serde(rename_all = "SCREAMING_SNAKE_CASE")`.
- Borrado: lógico (`deleted_at`) para activos con historial; físico solo para datos efímeros.
- Toda tabla nueva incluye `created_utc_ms`/`updated_utc_ms` cuando aplica.

---

## 4. Estrategia de entrega por fases

> Cada fase termina con un **demo reproducible**, pruebas automatizadas y un **criterio de salida medible**. No se abre la fase siguiente si el criterio anterior no se cumple.

### Fase 0 — Fundaciones y reducción de riesgo (2-3 semanas)

**Objetivo:** dejar el repositorio, la arquitectura y las decisiones críticas validadas antes de escribir features.

Tareas:

- [x] `git init`, estructura de monorepo, `justfile`, `.editorconfig`, licencia y política de contribución.
- [x] Workspace Rust con crates vacíos y reglas de dependencia; lints (`clippy -D warnings`), `rustfmt`.
- [x] CI base: Windows/macOS/Ubuntu, caché, `nextest`, `clippy`, `rustfmt`, `cargo-deny`, `cargo-audit`; frontend `tsc`, ESLint, Prettier, Vitest.
- [x] ADR-001 a ADR-012 (ver anexo C), con revisión legal de licencias (G-17).
- [x] Contratos IPC: DTOs versionados, códigos de error, esquema de eventos; generación de tipos TS (`ts-rs` o `specta`).
- [x] Migración base SQLite + runner + seed de categorías/emisora demo.
- [x] **Spike A (audio):** Tauri + CPAL + Symphonia reproducen MP3/FLAC a un *null sink*; medir latencia, underruns y CPU con 4 decks.
- [ ] **Spike B (video):** libmpv embebido vs. ventana nativa en Windows/macOS/Linux; medir tamaño, arranque y Wayland.
- [x] **Spike C (búsqueda):** 50.000 pistas sintéticas, FTS5 + trigram; medir P95 y ajustar tokenizer/índices.
- [ ] **Spike D (licencias/empaquetado):** build LGPL de libmpv/FFmpeg, SBOM, tamaño real del instalador; revisar RNF-05.
- [x] Design system: tokens del prototipo, tema claro/oscuro/alto contraste, componentes base accesibles.
- [x] Shell de la app con layout del prototipo (sin lógica real), navegación y atajos.

**Entregables:** repo + CI verde, ADRs aprobados, spikes con informe, migración base, shell UI.

**Criterio de salida:** los 4 spikes tienen decisión documentada; el build multiplataforma genera instaladores; CI <10 min; ningún blocker legal abierto.

**Esfuerzo:** ~6 personas-semana.

---

### Fase 1 — Núcleo: biblioteca y reproducción (6-7 semanas)

**Objetivo:** primer *vertical slice* real: importar, buscar, encolar, reproducir, medir y auditar.

Tareas:

- [ ] Dominio: `MediaAsset`, `Playlist`, `PlaylistItem`, `Station`, `User`, value objects (`CuePoints`, `Loudness`, `Duration`, `Checksum`).
- [ ] Casos de uso: `ImportMedia`, `SearchLibrary`, `CreatePlaylist`, `EnqueueItem`, `Play`, `Pause`, `Stop`, `Seek`, `Skip`.
- [ ] Ingesta: escaneo de carpeta, lectura de tags (`lofty`), checksum, duración, waveform peaks, detección de BPM/silencios (batch básico).
- [ ] Repositorios SQLite + FTS5 + migraciones; búsqueda con filtros (categoría, artista, género, año, BPM, tags).
- [ ] Motor de audio v1: un deck, play/pause/stop/seek, salida CPAL, decoder Symphonia, ring buffers, underrun logging.
- [ ] Cola secuencial, “stop al final”, reproducción de 10 pistas sin dropouts.
- [ ] UI: biblioteca virtualizada, búsqueda instantánea, cola, now playing, transporte, VU básico, reloj.
- [ ] Telemetría: posición, duración, estado, underruns; eventos a 20-30 Hz.
- [ ] Auditoría: `playout_logs` por cada inicio/fin, con origen `MANUAL`.
- [ ] Tests: dominio, repositorios con SQLite temporal, decode con corpus, búsqueda P95, E2E de import→play.

**Entregables:** app que reproduce una playlist real de 10 pistas, con biblioteca buscable y auditoría.

**Criterio de salida:**
- 10 pistas consecutivas, 60 min, **0 underruns** en hardware de referencia.
- Búsqueda P95 <15 ms con 50.000 filas.
- Arranque <2,5 s (medido; objetivo RNF-04 se ajusta tras spike).
- Cobertura dominio ≥85%, aplicación ≥75%.
- Recuperación tras `kill -9`: reabre y ofrece reanudar en <10 s.

**Esfuerzo:** ~26 personas-semana.

---

### Fase 2 — Emisión asistida: cart wall, crossfade y consola (6-7 semanas)

**Objetivo:** operación en vivo de una hora sin intervención técnica.

Tareas:

- [ ] Motor de dos decks con crossfade de potencia constante, cue points (`cue_in`, `intro`, `mix`, `fade_out`, `cue_out`, `hook`, `outro`).
- [ ] Detección automática de mix point (silencio a -14 dBFS) con override manual y previsualización.
- [ ] Cart wall: páginas, pads, hotkeys F1-F16, loop, gain por pad, stop individual/global, latencia P95 ≤30 ms.
- [ ] Instant Play y “PlayOver” (voz sobre pista con fade), “TalkOver” con ducking (ataque 20 ms, release 300 ms, -14 dB).
- [ ] Consola: EQ 12 bandas, compresor, limiter true peak, ganancia por canal, medidores L/R con peak hold, LUFS/true peak.
- [ ] Entrada de micrófono y auxiliar; selección de devices; perfil de latencia; corrección de deriva.
- [ ] Atajos configurables, modo operación vs. configuración, bloqueo de UI en aire.
- [ ] Persistencia de cue points y ganancias; snapshots de estado cada 5 s.
- [ ] Tests de audio: energía de crossfade ±0,5 dB, loudness -23 LUFS ±0,5, true peak ≤-1 dBTP, latencia de cart, soak 8 h.

**Entregables:** emisión asistida de 1 hora con cart wall, micrófono y crossfade; métricas visibles.

**Criterio de salida:**
- 1 hora continua sin underruns ni artefactos, con 4 carts + micrófono + EQ.
- Latencia de cart P95 ≤30 ms (Win/macOS), ≤50 ms (Linux).
- CPU <25% en el escenario de RNF-03; RAM ≤150 MB sin video.
- Todos los tests de audio en verde en CI con null sink.

**Esfuerzo:** ~26 personas-semana.

---

### Fase 3 — Automatización: scheduler, rotación y tráfico (8-9 semanas)

**Objetivo:** emisión automatizada 24 h con auditoría y pauta comercial.

Tareas:

- [ ] Modelo y repositorios de scheduler: eventos `AT/RECURRING/DTMF/MANUAL/API`, acciones, recurrencias, prioridad, dayparts.
- [ ] Runtime del scheduler: evaluación cada 500 ms, lookahead 30 s, materialización en cola con origen, manejo de colisiones y *catch-up* tras suspensión.
- [ ] Motor de rotación: filtros por categoría/género/artista/década/año/BPM/tags, cuotas, pesos, separación (artista/título/álbum), anti-repetición, fallback relajado con log.
- [ ] Generador de playlists por plantilla: slots, jingles, comerciales, hour markers, generación por horas/día, pre-generación.
- [ ] Tráfico: campañas, spots, vigencia, prioridad, topes hora/día, compilación de tandas, `ad_airings`, reemplazo local de publicidad en relay.
- [ ] HTH (hora/temperatura/humedad) sobre cortina con proveedor meteorológico detrás de puerto y caché.
- [ ] Modo AUTOMÁTICO/ASISTIDO, “stop al final”, forzar entrada, skip con motivo.
- [ ] Recuperación: snapshot + journal, reanudación de tanda/cola, reconciliación de `ad_airings` tras caída.
- [ ] UI: calendario semanal con drag-and-drop, editor de reglas, editor de campañas, vista de próximos eventos, alertas.
- [ ] Reportes v1: canciones y spots por rango, CSV/XLSX, consultas <200 ms.
- [ ] Tests: property-based de separación/rotación (nunca viola reglas duras), simulación de 30 días con reloj virtual, caos (archivo faltante, DB ocupada, reloj salta).

**Entregables:** automatización de 24 h con parrilla, rotación, tandas y reporte de aire.

**Criterio de salida:**
- Simulación de 30 días: 0 violaciones de separación dura, 0 spots vencidos emitidos, 100% de tandas auditadas.
- Emisión real de 24 h sin intervención; recuperación tras corte <10 s.
- Reporte XLSX/CSV conciliado 1:1 con `playout_logs`.
- Cobertura de dominio ≥90% en reglas de rotación/tráfico.

**Esfuerzo:** ~34 personas-semana.

---

### Fase 4 — Broadcast: streaming, metadatos, logger y señal (8-9 semanas)

**Objetivo:** operación de broadcast completa con salida a internet y trazabilidad.

Tareas:

- [ ] Encoder y cliente Icecast/Shoutcast v2: conexión, reintentos con backoff, buffer, TLS, `icy-metadata`, “now playing”.
- [ ] Exportación de metadatos a HTTP/XML/FTP/archivo; plantillas y throttling.
- [ ] RDS: puerto `RdsEncoder` con UECP y ASCII serie/TCP; PS/RT; simulador y pruebas de integración.
- [ ] DTMF: recepción en aux (Goertzel, ITU-T Q.23), generación, mapeo a eventos, debounce, log.
- [ ] Relay/retransmisión: fuente de device/stream/satélite, switch local, reemplazo de publicidad, time-delay/clipping configurable.
- [ ] Logger: grabación continua por fuente, rotación, retención, índice y reproducción desde la UI.
- [ ] Reportes PDF (XLSX ya en F3), plantillas de reporte de aire y comercial.
- [ ] Monitoreo: estado de streaming, nivel de buffer, reconexiones, alertas; panel de diagnóstico.
- [ ] Seguridad: secretos en keyring, permisos de exportación, auditoría de acciones.
- [ ] Tests: servidor Icecast local en CI, simulador RDS, DTMF sintético, corte de red, reloj, 72 h soak.

**Entregables:** emisión local + streaming + metadatos + logger + RDS/DTMF + reportes.

**Criterio de salida:**
- Stream estable 24 h con reconexión automática <5 s tras caída de red.
- Metadata correcta en Icecast/Shoutcast (verificada por lector externo).
- DTMF: 100% de detección en corpus sintético, 0 falsos positivos en 24 h de audio.
- Logger: 24 h continuas sin huecos; reproducción y exportación funcionando.
- Reporte PDF/XLSX con totales conciliados.

**Esfuerzo:** ~34 personas-semana.

---

### Fase 5 — Red, multiusuario y escala (8-9 semanas)

**Objetivo:** dos emisoras sincronizadas desde un servidor, con roles y API.

Tareas:

- [ ] Modo servidor: Axum, autenticación, RBAC, API REST v1 documentada (OpenAPI), rate limiting, auditoría.
- [ ] Multi-emisora: motores aislados por estación, cuotas de recursos, selección de estación en UI.
- [ ] Terminales remotas: descubrimiento, pairing, reconexión, estado de conexión; *single writer* en servidor.
- [ ] Sync de programación: versionado optimista, resolución de conflictos por timestamp/versión, tombstones; CRDT fuera de v1.
- [ ] Backups automáticos, restauración verificada, retención y exportación.
- [ ] Actualizaciones: updater firmado, canales dev/beta/stable, rollback, migración de DB con backup previo.
- [ ] Reportes consolidados multi-emisora; API para apps móviles (RF-41).
- [ ] Endurecimiento: threat model, pentest básico, CSP, dependencias, SBOM/provenance.
- [ ] Tests: carga 10 terminales, caos de red, upgrade/downgrade de DB, restauración.

**Entregables:** v1.0: servidor + terminales + multi-emisora + API + updater.

**Criterio de salida:**
- 2 emisoras y 10 terminales operando 24 h con consistencia verificada.
- API con contrato OpenAPI y tests de contrato.
- Restauración completa desde backup en <15 min, ensayada.
- Upgrade con rollback automático probado en las 3 plataformas.

**Esfuerzo:** ~34 personas-semana.

---

### Fase 6 — Video, radio visual y extensiones (opcional, 6-10 semanas)

**Objetivo:** completar RF de video y diferenciadores (v1.1/v2).

Tareas:

- [ ] Reproducción de video en ventana dedicada, playlist mixta audio/video, audio de video como fuente.
- [ ] Radio visual: composición de cámara + overlays (título, artista, hora) y salida a pantalla/stream.
- [ ] Editor de audio integrado (RF-15): waveform, corte, fade, normalización, time-stretch, ripping/CDDB.
- [ ] Plugins VST/WinAMP con aislamiento de proceso y sandbox (evaluar licencias).
- [ ] IA de voz/cápsulas y reconocimiento musical en la nube detrás de puertos; proveedor configurable.
- [ ] SCTE-35 (lectura/registro) y mayor cobertura de hardware RDS.
- [ ] Accesibilidad avanzada, temas y layouts guardables.

**Criterio de salida:** video estable sin afectar audio; editor y plugins con cobertura de pruebas; revisión de licencias completada.

**Esfuerzo:** ~30-45 personas-semana (según alcance).

---

## 5. Cronograma y hitos

Con el equipo base (≈4,5 FTE), la secuencia F0-F5 suma **~160 personas-semana**. Con 20% de contingencia: **~190 personas-semana ≈ 42 semanas calendario** hasta v1.0.

| Hito | Fin de | Resultado demostrable | Semana estimada |
|---|---|---|---|
| M0 | F0 | Repo, CI, ADRs, spikes, shell UI | 3 |
| M1 | F1 | Importar → buscar → reproducir 10 pistas | 9 |
| M2 | F2 | 1 hora al aire con cart wall, mic y crossfade | 16 |
| M3 | F3 | 24 h automatizadas con tandas y reporte | 25 |
| M4 | F4 | Streaming, RDS/DTMF, logger y reportes | 34 |
| M5 | F5 | Multi-emisora, terminales, API, updater → **v1.0** | 42 |
| M6 | F6 | Video/radio visual, editor, plugins → v1.1 | +8-12 |

Notas:

- Un equipo de 2 personas multiplica el calendario por ~2; un desarrollo unipersonal por ~4-5. El plan prioriza *vertical slices* para que cada recorte sea demostrable.
- Las fases 3 y 4 pueden solaparse parcialmente (scheduler/tráfico vs. streaming) si hay dos frentes Rust.
- La fecha de v1.0 no debe comprometerse antes de cerrar los spikes de F0 (G-01, G-12, G-17).

---

## 6. Estrategia de pruebas y calidad

### 6.1 Pirámide y puertos de test

| Nivel | Herramienta | Cobertura objetivo | Qué valida |
|---|---|---|---|
| Unitario dominio | `cargo test` + `proptest` | ≥85% | Rotación, separación, dayparting, cue points, crossfade math, tandas. |
| Unitario aplicación | mocks de puertos | ≥75% | Casos de uso, errores, idempotencia, permisos. |
| Integración | SQLite temporal + null sink | ≥70% | Repositorios, migraciones, IPC, scheduler, tráfico. |
| Audio | corpus + WAV golden | 100% corpus | Decode, resample, loudness, crossfade, limiter, underruns. |
| E2E | Playwright + tauri-driver | Flujos críticos | Importar, emitir, programar, reportar, recuperar. |
| Caos | inyección | Escenarios | Archivo faltante, DB ocupada, device desconectado, red caída, disco lleno. |
| Soak | 72 h aceleradas | 0 leaks/underruns | Estabilidad y memoria. |

### 6.2 Pruebas de audio deterministas

- **Null sink/source** implementando `AudioSinkPort`/`AudioSourcePort`; el motor no sabe si es real o nulo. Esto permite CI en Linux sin ALSA.
- **Corpus licenciado** de 20-30 archivos: WAV 44.1/48/96 kHz, MP3 CBR/VBR, FLAC, AAC/M4A, OGG, AIFF, silencios, clipping, mono/estéreo/5.1, archivos corruptos y con tags raros.
- **Golden PCM:** decode → hash de samples (tolerancia ±1 LSB) para detectar regresiones.
- **Métricas:** integrated LUFS (-23 ±0,5), true peak (≤-1 dBTP), LRA, energía de crossfade (±0,5 dB), latencia P95, underruns.
- **Benchmarks:** `criterion` para decode, cadena DSP, búsqueda y scheduler; umbrales en CI.

### 6.3 Pruebas de dominio críticas

- Property-based: para cualquier combinación de reglas, el generador **nunca** viola separación dura ni dayparting; si no hay candidatos, relaja en orden documentado y registra.
- Simulación de 30 días con reloj virtual y semilla fija → resultado reproducible.
- Tandas: ningún spot vencido/inactivo; topes horarios/diarios respetados; prioridad y peso deterministas.
- Recuperación: snapshot + journal reconstruye exactamente la cola y la tanda en curso.

### 6.4 Accesibilidad e i18n

- `axe-core` en CI sobre las vistas principales; navegación solo teclado; foco visible; contraste AA.
- Waveform con descripción textual, marcas accesibles y seek por teclado; `aria-live` para “en aire”, alertas y cambios de pista.
- Pruebas de pseudo-localización (textos +40%) y formatos de fecha/número por locale.
- Atajos configurables y documentados; conflicto de atajos detectado al guardar.

---

## 7. CI/CD, empaquetado y operación

### 7.1 Pipeline

1. **PR:** `fmt` → `clippy -D warnings` → `nextest` → frontend `tsc/eslint/vitest` → `cargo-deny`/`audit` → build de debug.
2. **Main:** integración + audio smoke + benchmarks (no bloqueantes al inicio, sí con presupuesto) + build multiplataforma.
3. **Release:** `tauri build` firmado, notarización macOS, MSIX/NSIS Windows, AppImage/deb/rpm Linux, SBOM (Syft), provenance, updater manifest, changelog.
4. **Nightly:** soak 8 h, E2E completo, escaneo de dependencias, restore de backup.

### 7.2 Distribución

- Canales `dev`, `beta`, `stable`; versionado semántico; migraciones de DB con backup obligatorio y rollback.
- Firma: Windows (EV/Azure Trusted Signing), macOS (Developer ID + notarización), Linux (firma de repositorio).
- Instalador con componentes: núcleo; libmpv/FFmpeg opcional según ADR-007; verificación de integridad.

### 7.3 Runbooks mínimos

- Instalación y primer arranque; configuración de devices y latencia.
- Configuración de Icecast/Shoutcast y metadatos; diagnóstico de reconexión.
- Programación de parrilla y tandas; resolución de conflictos.
- Recuperación tras corte de energía; restauración de backup.
- Rotación de secretos; actualización y rollback; recolección de diagnóstico.

---

## 8. Riesgos y mitigaciones

| ID | Riesgo | Prob. | Impacto | Mitigación | Fase |
|---|---|---|---|---|---|
| R-01 | Glitches de audio por WebView/OS o devices | Media | Alto | Null sink en CI, buffer configurable, watchdog, pruebas en hardware real, perfil de latencia. | F0-F2 |
| R-02 | libmpv inestable/embebido en Wayland | Media | Alto | Spike F0; ventana nativa como fallback; feature flag para deshabilitar video. | F0/F6 |
| R-03 | Licencias GPL/FFmpeg/VST/ASIO | Alta | Alto | Revisión legal temprana, build LGPL, enlace dinámico, SBOM, alternativas GStreamer. | F0 |
| R-04 | RNF-05 (<25 MB) incumplible | Alta | Medio | Renegociar meta a ≤80 MB o componentes opcionales; comunicar trade-off. | F0 |
| R-05 | Scheduler con deriva/horas mal manejadas | Media | Alto | UTC epoch, `chrono-tz`, reloj virtual en tests, simulación 30 días, alertas de desvío. | F3 |
| R-06 | Rotación que repite o viola reglas | Media | Alto | Property-based, separación dura, fallback auditado, panel de diagnóstico. | F3 |
| R-07 | Tráfico: spots vencidos o doble emisión | Media | Alto | `ad_airings` único, validación al compilar, reconciliación al arrancar. | F3 |
| R-08 | Sync multi-terminal inconsistente | Media | Alto | Single writer, versionado optimista, tombstones; CRDT diferido. | F5 |
| R-09 | Crecimiento de alcance (video, IA, VST) | Alta | Medio | Feature flags, fases 6 separada, criterios de salida estrictos. | Todas |
| R-10 | Deuda por prototipo CDN/mock | Baja | Bajo | Reescritura desde F0; el prototipo es contrato visual, no base de código. | F0 |
| R-11 | Corrupción de DB o backups inválidos | Baja | Alto | WAL, `VACUUM INTO`, restore test semanal, checksums, migraciones probadas. | F1+ |
| R-12 | Equipo pequeño / conocimiento Rust | Media | Medio | Spikes, pair programming, documentación, contratación temprana, alcance por fases. | Todas |

---

## 9. Gobernanza y definición de terminado

### 9.1 Definition of Ready (por historia)

- RF/HU vinculada y criterio de aceptación medible.
- Contrato IPC/DB definido (DTO o migración).
- Puerto/adaptador identificado y estrategia de test.
- Impacto en audio/tiempo real evaluado (allocations, locks, latencia).
- UX acordada (wireframe o componente del design system).
- Riesgos y dependencias registrados.

### 9.2 Definition of Done

- Código con `clippy`/`rustfmt`/ESLint/Prettier limpios; sin `unwrap()`/`expect()` en producción.
- Tests unitarios + integración; cobertura no disminuye.
- Migración probada en base vacía y en base con datos de la versión anterior.
- Telemetría y códigos de error añadidos; logs sin datos sensibles.
- Documentación: rustdoc en dominio, comando documentado, ADR si aplica.
- Demo en las 3 plataformas (o justificación documentada).
- Sin regresiones en audio smoke ni en benchmarks clave.

### 9.3 Métricas de ingeniería

| Métrica | Objetivo |
|---|---|
| Cobertura dominio / aplicación | ≥85% / ≥75% |
| Underruns en soak 24 h | 0 |
| Latencia de comando P95 | ≤50 ms (cart ≤30 ms Win/macOS) |
| Búsqueda FTS P95 (50k) | <15 ms |
| Arranque | <2,5 s (objetivo revisado) |
| Memoria en reposo (audio) | ≤150 MB |
| CPU escenario RNF-03 | <25% |
| Tiempo de recuperación | <10 s |
| Defectos críticos abiertos al release | 0 |

### 9.4 ADRs iniciales

| ADR | Tema |
|---|---|
| ADR-001 | Framework UI: React 19 + TypeScript. |
| ADR-002 | Motor de audio dual-deck: CPAL + Symphonia + DSP propio. |
| ADR-003 | Video: libmpv, ventana nativa vs. superficie embebida; audio de video. |
| ADR-004 | SQLite bundled + FTS5 + escritor único + migraciones. |
| ADR-005 | Modelo de tiempo UTC epoch ms y zonas. |
| ADR-006 | Errores por capa y contrato IPC versionado. |
| ADR-007 | Licencias y empaquetado de libmpv/FFmpeg; tamaño del instalador. |
| ADR-008 | Secretos con keyring del SO. |
| ADR-009 | Scheduler: evaluación, lookahead y recuperación. |
| ADR-010 | Sync v1: single writer y concurrencia optimista (CRDT diferido). |
| ADR-011 | Telemetría UI 20-30 Hz y agregación. |
| ADR-012 | Plugins fuera de v1.0 y aislamiento futuro. |

---

## 10. Primeros 10 días (acción inmediata)

| Día | Actividad | Salida |
|---|---|---|
| 1 | `git init`, workspace, `justfile`, CI esqueleto, `.gitignore`. | Repo compilando con crates vacíos. |
| 2 | ADR-001/002/004/007 en borrador; revisión de licencias; threat model inicial. | Decisiones y riesgos registrados. |
| 3-4 | **Spike A:** CPAL + Symphonia + null sink; medir latencia, CPU, underruns con 4 decks. | Informe con números y recomendación. |
| 5-6 | **Spike B:** libmpv en Tauri (Win/macOS/Linux); tamaño, arranque, Wayland. | Decisión de superficie de video. |
| 7-8 | **Spike C:** migración base + FTS5 con 50k filas; benchmark P95. | Migración 0001 + informe de búsqueda. |
| 9-10 | Vertical slice mínimo: importar un archivo, listarlo, encolarlo, reproducirlo, ver medidores y log. | Demo + go/no-go para F1. |

**Criterio de go/no-go:** si el slice reproduce 10 pistas sin underruns, la búsqueda cumple P95 y el build multiplataforma es viable, se aprueba F1 con el alcance propuesto. Si falla el audio o el empaquetado, se revisan ADR-002/003/007 antes de continuar.

---

## Anexo A — Trazabilidad RF → fase

| RF | Fase | Nota |
|---|---|---|
| RF-01, RF-02 | F1 (audio) / F6 (video) | Formatos audio en F1; video en F6. |
| RF-03 | F6 | Audio de video como fuente. |
| RF-04, RF-05 | F2 | Cart wall y reproductor principal. |
| RF-06, RF-07 | F2 | TalkOver/PlayOver. |
| RF-08, RF-09 | F2 | Cue points y mix point. |
| RF-10, RF-11, RF-13 | F1 | Biblioteca, metadatos, categorías. |
| RF-12 | F1/F2 | Batch básico en F1; completo con editor en F6. |
| RF-14 | F1/F3 | Hot folders/WhatsApp en F3. |
| RF-15 | F6 | Editor de audio. |
| RF-16 a RF-24 | F3 | Scheduler, rotación, dayparting, HTH, tandas. |
| RF-25 a RF-28 | F2 | Crossfade, EQ, consola, mix virtual. |
| RF-29 | F6 | Plugins. |
| RF-30, RF-31 | F4 | DTMF y logger. |
| RF-32 | F4 | Time-delay/clipping. |
| RF-33, RF-34 | F4 | Streaming y metadatos. |
| RF-35 | F4 | RDS. |
| RF-36 | F6 | IA de voz. |
| RF-37 | F5 | RBAC y perfiles. |
| RF-38 | F5 | Multi-emisora. |
| RF-39 | F5 | API remota. |
| RF-40 | F3/F4 | Reportes CSV/XLSX en F3; PDF en F4. |
| RF-41 | F5 | API para apps móviles. |

## Anexo B — Historias pendientes de redactar antes de su fase

Antes de F3: HU de scheduler (crear/editar/recurrencia/DTMF), rotación (filtros/separación/fallback), generador de playlists, tanda (vigencia/topes/prioridad), HTH, recuperación.

Antes de F4: HU de streaming (conexión/reconexión/metadata), RDS, DTMF RX/TX, relay/reemplazo local, logger, reportes PDF.

Antes de F5: HU de multiusuario/RBAC, multi-emisora, terminal remota, API, backup/restore, updater.

Cada HU debe incluir criterios medibles (latencia, precisión, recuperación) y su vínculo a RF/RNF.

## Anexo C — Glosario

- **Cue In / Cue Out:** inicio y fin audibles de la pista.
- **Intro:** segmento instrumental antes de la voz.
- **Mix/Next:** punto donde debe arrancar la siguiente pista.
- **Hook:** fragmento representativo para ráfagas.
- **Crossfade de potencia constante:** curva cos/sin que mantiene la energía percibida.
- **Ducking/TalkOver:** atenuación automática de la música al hablar.
- **Dayparting:** restricción de contenidos por franja horaria.
- **HTH:** hora, temperatura y humedad sobre cortina.
- **LUFS / dBTP / LRA:** sonoridad integrada, pico real y rango de sonoridad.
- **UECP:** protocolo de control de codificadores RDS.
- **SNC:** señal de control de red/satélite usada para disparar eventos.
- **Null sink:** salida de audio de prueba que descarta samples y permite CI determinista.

---

**Fin del plan.**
