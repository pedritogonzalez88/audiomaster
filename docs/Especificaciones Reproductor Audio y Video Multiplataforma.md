# Especificaciones Funcionales y Técnicas — Reproductor Multimedida Audio y Video Desktop Multiplataforma

**Proyecto:** Automatizador y Reproductor Multimedida para Broadcast, Streaming y Producción.
**Versión del documento:** 1.0.
**Autor:** Pedro Daniel González Meza.
**Arquitectura de Referencia:** Clean Architecture + Capa de Presentación Web (Tauri).

---

## 1. Contexto y Objetivo

Este documento define los requisitos funcionales (RF), no funcionales (RNF), de experiencia de usuario (UX) / interfaz (UI), arquitectura limpia y estándares de código limpio para un reproductor de audio y video para escritorio, multiplataforma (Windows, macOS, Linux) con funcionalidad equivalente a las suite comerciales de automatización de emisoras radiofónicas y estudio de radio/TV:

- **Dinesat (Hardata)** — [www.dinesat.com](https://www.dinesat.com/home_es.html)
- **RadioBoss (DJSoft)** — [www.djsoft.net](https://www.djsoft.net/enu/products/radioboss/radioboss.htm)

Se investigó la funcionalidad actual de ambas referencias (noviembre 2026) para construir un árbol funcional completo, documentando la fuente de cada bloque funcional.

## 2. Investigación — Consolidado funcional

### 2.1 RadioBoss

RadioBoss es la plataforma de automatización de radio con licencia comercial ampliamente difundida, actualmente en versión 7.2.x. Las funciones publicadas en el sitio oficial se consolidan en [RadioBoss Overview](https://www.djsoft.net/enu/description_radioboss.htm) y en la [página de producto](https://www.djsoft.net/enu/products/radioboss/radioboss.htm), y se agrupan aquí en módulos:

| Módulo | Funciones |
|---|---|
| Reproductor / Playlists | Generador de playlist con plantillas, mezcla continua (crossfade), generador aleatorio, pre-generación por horas, captura de audio desde fuente, reproducción secuencial, mezcla de entrada, generador de cortinas/jingles con teclado |
| Programación (Scheduler) | Scheduler con eventos por fecha/hora, disparo por señal DTMF, mezcla automática de tandas, inserción automática de jingles |
| Advertising / Anuncios | Scheduler comercial, reproducción con volumen independiente para anuncios, gestión remota de anuncios sobre red |
| Audio processing | Nivelación automática, crossfade, silencios removidos, ecualizador 12 bandas, compresor, plugins WinAMP y VST, entrada de micrófono/lineal, ASIO/WASAPI, salidas multicanal (4.0, 5.1, 7.1), cart wall |
| Biblioteca musical | Indexación, tags, gestión de BPM, nivelación por lotes, filtros y búsqueda, etiquetas y anotaciones |
| Streaming / Metadata | Shoutcast, Icecast, WMServer, streaming integrado, exportación de metadatos a FTP, HTTP, XML |
| Reportes / Acceso remoto | Reportes detallados por rango de fecha, exportación a XLS/PDF, API de control remoto, control de acceso por usuario, perfiles |
| Novedades 7.2 | Mix Virtual, Layout Manager, Dayparting, actualizaciones a Scheduler y Playlist Generator, compresor multiband |

Fuente de cada bloque: [RadioBoss Features](https://www.djsoft.net/enu/products/radioboss/features.htm) y [RadioBoss Description](https://www.djsoft.net/enu/description_radioboss.htm).

### 2.2 Dinesat (Hardata)

Dinesat es la plataforma de automatización radiofónica producida por Hardata (Argentina), con versiones Dinesat, Dinesat Visual (radio visual, audio y video) y HDX Radio. Las funciones publicadas se detallan en [Dinesat Home](https://www.dinesat.com/home_es.html) y en la [ficha de Dinesat 13](https://www.dinesat.com/radio/es/), y se agrupan aquí en módulos:

| Módulo | Funciones |
|---|---|
| Contenido | Carga, programación y emisión de materiales de audio, video (radio visual), clasificación automática de metadatos, edición y emisión de audios de WhatsApp, catalogación automática de metadatos |
| Automatización | Programación musical automática por filtros (género, artista, década), detección automática de punto de mezcla, generación de musicalización prolija, horarios/clima/hora, HTH (hora/temperatura/humedad) sobre cortina musical |
| Emisión / IPlay | 4 reproductores simultáneos, Instant Play, cart wall de 100 botones por página, reproducción en loop, marcas punch in/out, emisión instantánea, reproducción de audio de archivos video |
| Tanda / Publicidad | Emisión automática de publicidad, reemplazo de publicidad local en señales retransmitidas, programación de anuncios para emisoras afiliadas |
| Red / Multi-emisora | Hasta 4 emisoras simultáneas (Dinesat 13), retransmisión de hasta 10 señales, hasta 10 terminales en red, versiones ilimitadas en HDX |
| Auditoría / Trazabilidad | Auditoría de emisión, logging de señales, grabación continua de múltiples señales, reportes de aire |
| Contenido interactivo | Generación automática de noticias/cápsulas, capacidad de edición y emisión de audios de WhatsApp, contestador telefónico, módulos telefónicos (ranking, votaciones, mensajes) |
| IA / Inteligencia | Herramienta IA para voces hiperrealistas, IA de generación de cápsulas informativas, servicios en la nube (DCS) para reconocimiento de temas musicales |
| Visual / Video | Integración de contenido de video en radio visual, video playback integrado |
| Otras funciones | Detección de DTMF, edición de señales SNC, audios de señales satelitales, plugins RDS para RDS/TMC |

### 2.3 Análisis técnico del stack multiplataforma

Investigación del panorama actual de frameworks desktop (noviembre 2026), comparando tamaño, rendimiento, curva de Rust vs Dart vs JS, soporte de audio/video:

| Criterio | Tauri 2 | Electron | Flutter Desktop | Qt / QML |
|---|---|---|---|---|
| Backend | Rust | Node.js | Dart | C++ / Python (PySide) |
| Tamaño mínimo | ~3–12 MB | ~80 MB | ~20 MB | ~10 MB |
| Memoria en reposo | 30–50 MB | 200–400 MB | 100–200 MB | ~50 MB |
| Linux / Webview | Fragile en WebKit (según revisión [dev.to/huzaifa207](https://dev.to/huzaifa207/electron-vs-tauri-how-to-choose-for-your-next-desktop-app-392e)) | Excelente | Buena | Excelente |
| Acceso a servicios de sistema | Crates Rust | Node APIs | Plugins | C++ / bindings |
| Popularidad | Creciente ([dev.to/isertter](https://dev.to/iserter/i-asked-gemini-to-compare-the-best-cross-platform-desktop-app-frameworks-and-here-are-the-results-117l)) | Dominante | Fuerte | Maduro |

### 2.4 Backend de audio/video (referencia Rust)

Para la capa media de audio/video se investigaron librerías Rust nativas:

- **CPAL** — librería pura Rust para I/O de audio PCM, soporte para entradas y salidas, API no compleja pero de bajo nivel ([reddit/r/rust/x2gd8n](https://www.reddit.com/r/rust/comments/x2gd8n/maybe_this_is_a_dumb_question_but_im_a_bit/)).
- **Rodio** — librería de alto nivel sobre CPAL, soporte para decodificación, mixing y efectos; actualmente usa Symphonia como backend por defecto ([reddit/r/rust/1lyobv2](https://www.reddit.com/r/rust/comments/1lyobv2/announcing_rodio_021/)).
- **Symphonia** — decodificadores puros Rust para MP3, WAV, FLAC, AAC; rendimiento comparable a FFmpeg ([reddit/r/rust/oa6539](https://www.reddit.com/r/rust/comments/oa6539/symphonia_v03_purerust_decoders_for_mp3_wav_flac/)).
- **libmpv** — librería madura para reproducción de audio y video, utilizada en apps de producción (ejemplo: Kinema, construida con Tauri 2 + React + libmpv para reproducción local con HDR y soporte de canales ATMOS) ([reddit.com/r/foss/1xe6acj](https://www.reddit.com/r/foss/comments/1xe6acj/i_wanted_a_fast_clean_way_to_watch_my_media/)).
- **GStreamer / ffmpeg-next** — alternativas multi-codec con soporte de video y audio de alta calidad, ambas utilizadas en apps de radio/TV producción.

**Decisión recomendada (ver §5):** Tauri 2 + Rust + libmpv como backend de reproducción de video y Symphonia/CPAL como fallback para procesamiento de audio de precisión (crossfade, fades, DTMF, mixing).

## 3. Visión del Producto

Un reproductor de audio/video para escritorio multiplataforma (Windows, macOS, Linux) con:

1. **Motores de reproducción paralelos** — cart wall, reproductor principal, slots de Instant Play, emisión de tandas comerciales.
2. **Automatización de emisión** — playlists programadas por fecha/hora, disparo de eventos, rotación musical por filtros, emisión de tandas, pautado comercial.
3. **Procesamiento de audio** — crossfade automático con detección de punto de mezcla, nivelación, EQ, compresor, TalkOver, entrada de micrófono.
4. **Soporte multi-emisora / red** — centralización de pautado, terminales remotas, sincronización con nodos satelitales (SNC/DTMF).
5. **Registro de aire** — auditoría de emisión, reportes por rango de fechas, exportación a XLS/PDF.
6. **Streaming** — salida a Shoutcast/Icecast, metadatos ahora emitidos, captura de entradas de radio/TV.
7. **Acceso remoto / Multiusuario** — perfiles, control remoto via API/REST, multi-emisora con separación de contenido.

## 4. Requisitos Funcionales (RF)

### 4.1 Reproducción — Motor de Audio/Video

| ID | Requisito | Fuente |
|---|---|---|
| RF-01 | Reproducción de archivos de audio y video con soporte nativo multiformato (MP3, AAC, FLAC, OGG, WAV, AIFF, M4A; video: MP4, MKV, AVI, MOV, WebM) | RadioBoss ([feature](https://www.djsoft.net/enu/description_radioboss.htm)) |
| RF-02 | Decodificación con codificación dinámica (hardware-accelerated), así como fallback a software | Referencia libmpv/GStreamer ([reddit/foss](https://www.reddit.com/r/foss/comments/1xe6acj/i_wanted_a_fast_clean_way_to_watch_my_media/)) |
| RF-03 | Reproducción de audio de archivos de video como fuente de emisión de radio | Dinesat ([home](https://www.dinesat.com/home_es.html)) |
| RF-04 | Cart Wall con reproductores independientes, reproducción instantánea, páginas de botones personalizables, marcas punch in/out, loop | Dinesat IPlay ([página](https://www.dinesat.com/iplay/es/)) |
| RF-05 | Reproductor principal con reproducción secuencial, pausa, stop al final, indicador de siguiente título, reproducción de playlist continuo | Dinesat ([manual](https://es.scribd.com/document/89408294/Hardata-Dinesat-Radio-Quick-Guide-Spanish)) |
| RF-06 | TalkOver — atenuación suave de música para insertar voz en vivo | Dinesat ([manual](https://es.scribd.com/document/89408294/Hardata-Dinesat-Radio-Quick-Guide-Spanish)) |
| RF-07 | PlayOver — inserción de una grabación sobre una pista en reproducción con fade-out | Dinesat ([manual](https://es.scribd.com/document/89408294/Hardata-Dinesat-Radio-Quick-Guide-Spanish)) |
| RF-08 | Emisión de anuncios en etiquetas o en posición específica dentro del material | Dinesat ([comparativa](https://www.dinesat.com/compare_es.html)) |
| RF-09 | Detección automática del punto óptimo de mezcla entre archivos | Dinesat ([radio/es](https://www.dinesat.com/radio/es/)) |

### 4.2 Biblioteca de contenido

| ID | Requisito | Fuente |
|---|---|---|
| RF-10 | Biblioteca centralizada con indexación de archivos de audio y video, búsqueda por código, título, artista, álbum, género, década, año, BPM, etiquetas, palabras clave | RadioBoss ([features](https://www.djsoft.net/enu/products/radioboss/features.htm)), Dinesat ([radio/es](https://www.dinesat.com/radio/es/)) |
| RF-11 | Gestión de metadatos automática desde nube de reconocimiento (equivalente a DCS de Dinesat) y mediante tags de archivo | Dinesat ([radio/pt](https://www.dinesat.com/radio/pt/index.html)) |
| RF-12 | Procesamiento por lotes: normalización de volumen, detección de BPM, detección de silencios, recorte, time-stretching | RadioBoss ([features](https://www.djsoft.net/enu/description_radioboss.htm)), Hardata (editor) ([diarioti](https://diarioti.com/hardata-lanza-nuevo-software-de-automatizacion-radial/9587)) |
| RF-13 | Clasificación de contenidos por categorías, tipo, emisoras asignadas | Dinesat ([radio/es](https://www.dinesat.com/radio/es/)) |
| RF-14 | Importación automática de audios provenientes de WhatsApp, hot folders, descarga de mensajes | Dinesat ([radio/pt](https://www.dinesat.com/radio/pt/index.html)) |
| RF-15 | Editor de audio integrado con funciones comunes + recorte de silencios, normalización, time-stretching, ripping de CD, lectura de CDDB, importación batch | Hardata ([diarioti](https://diarioti.com/hardata-lanza-nuevo-software-de-automatizacion-radial/9587)) |

### 4.3 Programación y automatización

| ID | Requisito | Fuente |
|---|---|---|
| RF-16 | Scheduler de eventos por fecha/hora, con disparo condicional, soporte de DTMF | RadioBoss ([features](https://www.djsoft.net/enu/description_radioboss.htm)) |
| RF-17 | Rotación musical por filtros (artista, género, década, año, BPM, tags) con creación de programación completa automática | Dinesat ([radio/es](https://www.dinesat.com/radio/es/)) |
| RF-18 | Playlist Generator con plantillas, generación por horas, playlists aleatorias, playlists con jingles, comerciales, hour markers | RadioBoss ([features](https://www.djsoft.net/enu/description_radioboss.htm)) |
| RF-19 | Dayparting — restricción de contenidos por franja horaria | RadioBoss 7.2 ([features](https://www.djsoft.net/enu/products/radioboss/features.htm)) |
| RF-20 | Emisión de HTH (Hora, Temperatura, Humedad) sobre cortina musical | Dinesat ([comparativa](https://www.dinesat.com/compare_es.html)) |
| RF-21 | Automatización de clima/hora con integración a servicios meteorológicos | Dinesat ([home](https://www.dinesat.com/home_es.html)) |
| RF-22 | Emisión de tandas publicitarias con scheduling, fechas de inicio/fin, volumen independiente para anuncios | RadioBoss ([features](https://www.djsoft.net/enu/description_radioboss.htm)) |
| RF-23 | Retransmisión de señales externas con reemplazo de publicidad local (SNC, satelital) | Dinesat ([home](https://www.dinesat.com/home_es.html)) |
| RF-24 | Trazabilidad de bloque comercial emitido para auditoría (reporte de aire) | Dinesat ([comparativa](https://www.dinesat.com/compare_es.html)) |

### 4.4 Procesamiento de audio y consola

| ID | Requisito | Fuente |
|---|---|---|
| RF-25 | Crossfade automático, nivelación, eliminación de silencios | RadioBoss ([features](https://www.djsoft.net/enu/description_radioboss.htm)) |
| RF-26 | Ecualizador multibanda (12 bandas) + compresor de audio | RadioBoss ([features](https://www.djsoft.net/enu/description_radioboss.htm)) |
| RF-27 | Soporte de entrada de micrófono, entrada lineal, entrada satelital, WASAPI/ASIO en Windows, dispositivos multicanal 4.0/5.1/7.1 | RadioBoss ([features](https://www.djsoft.net/enu/description_radioboss.htm)) |
| RF-28 | Mix Virtual — consola virtual con mixer para mezclar pistas, efectos, micrófono | RadioBoss 7.2 ([features](https://www.djsoft.net/enu/products/radioboss/features.htm)) |
| RF-29 | Soporte de plugins VST y WinAMP | RadioBoss ([features](https://www.djsoft.net/enu/description_radioboss.htm)) |
| RF-30 | Detección y generación de tonos DTMF | RadioBoss ([features](https://www.djsoft.net/enu/description_radioboss.htm)), Dinesat ([comparativa](https://www.dinesat.com/compare_es.html)) |
| RF-31 | Grabación de señales entrantes (Logger, grabación continua de múltiples señales) | Dinesat ([radio/pt](https://www.dinesat.com/radio/pt/index.html)) |
| RF-32 | Time-delay / clipping para señales de producción / TV | Dinesat TV ([tv/es](https://www.dinesat.com/tv/es/)) |

### 4.5 Streaming y salida

| ID | Requisito | Fuente |
|---|---|---|
| RF-33 | Emisión a Shoutcast e Icecast, servidor de streaming integrado, soporte de metadatos "now playing" | RadioBoss ([features](https://www.djsoft.net/enu/description_radioboss.htm)) |
| RF-34 | Exportación de metadatos por FTP, HTTP request, XML, texto plano | RadioBoss ([features](https://www.djsoft.net/enu/description_radioboss.htm)) |
| RF-35 | Soporte RDS — transmisión de nombre de emisora, título e intérprete vía RDS (Radio Data System) | Hardata ([diarioti](https://diarioti.com/hardata-lanza-nuevo-software-de-automatizacion-radial/9587)) |
| RF-36 | Integración de voz por IA para generación de cápsulas/noticias | Dinesat ([radio/pt](https://www.dinesat.com/radio/pt/index.html)) |

### 4.6 Gestión, usuarios, red y reportes

| ID | Requisito | Fuente |
|---|---|---|
| RF-37 | Gestión de usuarios con roles, perfiles, entorno de trabajo personalizado por usuario | Dinesat ([manual](https://silo.tips/download/hardata-dinesat-radio-radio-automation-manual-de-usuario-version)), RadioBoss ([features](https://www.djsoft.net/enu/description_radioboss.htm)) |
| RF-38 | Soporte multi-emisora (hasta N emisoras simultáneas, terminales en red, retransmisión) | Dinesat ([comparativa](https://www.dinesat.com/compare_es.html)) |
| RF-39 | API de control remoto para integraciones (REST/gRPC) | RadioBoss ([features](https://www.djsoft.net/enu/description_radioboss.htm)) |
| RF-40 | Reportes de canciones emitidas / anuncios por rango de fechas, exportación a XLS, PDF | RadioBoss ([features](https://www.djsoft.net/enu/description_radioboss.htm)) |
| RF-41 | Control remoto por apps móviles (opcional vía API REST) | — |

## 5. Requisitos No Funcionales (RNF)

| ID | Requisito | Métrica / Meta |
|---|---|---|
| RNF-01 | Multiplataforma | Windows 10/11, macOS 13+, Linux (x86_64 y ARM64), sin recompilar la capa de dominio |
| RNF-02 | Rendimiento del motor de reproducción | Latencia de reacción < 50 ms (orden de mando a reproducción), sin artefactos audibles (dropouts) con ≥ 4 pistas simultáneas + micrófono |
| RNF-03 | Consumo de recursos | RAM en reposo ≤ 150 MB (Tauri/Rust, sin video activo), CPU < 5% en reproducción simple; < 25% con cart wall + EQ + crossfade activos |
| RNF-04 | Tiempo de arranque | < 2 s desde doble clic a UI interactiva |
| RNF-05 | Tamaño del paquete instalado | < 25 MB (sin incluir biblioteca de usuario) |
| RNF-06 | Estabilidad | Uptime de emisión 24/7 sin reinicio obligatorio; recuperación automática de estado (playlists en aire, posición) tras corte de energía |
| RNF-07 | Persistencia | Motor de base de datos local embebido (SQLite + FTS5) para biblioteca, logs, programación; backups automáticos con retención configurable |
| RNF-08 | Escalabilidad de red | Arquitectura cliente/servidor opcional: hasta 10+ terminales sincronizadas, consistencia eventual (CRDT) para cambios de programación concurrentes |
| RNF-09 | Seguridad | Autenticación por usuario, permisos granulares, control de acceso a emisoras, cifrado de credenciales del sistema operativo (Keychain, Secret Service, DPAPI) |
| RNF-10 | Actualizaciones | Actualización automática segura (firma digital, rollback), esquema de versiones semántico |
| RNF-11 | Accesibilidad | Conformidad WCAG 2.2 AA en UI web; navegación completa por teclado (atajos de operador de radio); alto contraste |
| RNF-12 | Internacionalización | Arquitectura i18n nativa desde v1: es-ES, es-419 (PY/AR), pt-BR, en-US; fechas/horas UTC + zona local |
| RNF-13 | Observabilidad | Logging estructurado con niveles, tracing distribuido opcional, métricas de salud del motor (buffer, underruns, latencia) |
| RNF-14 | Compatibilidad de formatos | Cobertura ≥ 95% de formatos comúnmente usados en emisoras (validado por suite de test con corpus de archivos) |
| RNF-15 | Testabilidad | Cobertura de pruebas unitarias de dominio ≥ 85%, integración ≥ 70%; suite de pruebas de humo de audio en CI con dispositivos virtuales (dispositivos nulos de audio) |

## 6. UX/UI

### 6.1 Principios de diseño

- **Densidad de información operativa** — la pantalla principal muestra todo lo que el operador necesita sin cambiar de ventana: estado de las pistas, reloj de emisión, próximo evento, VU meters.
- **Modo operación vs modo configuración** — la operación en aire es de respuesta inmediata (atajos y botones grandes); la configuración se maneja en vistas separadas sin interrumpir la emisión.
- **Consistencia** — mismo motor de componentes en toda la UI (design system propio), espaciado y tipografía coherentes.

### 6.2 Layout principal

```
┌────────────────────────────────────────────────────────────┐
│ Barra superior: nombre emisora, reloj, estado ON AIR,        │
│ indicadores (streaming, logger, entradas), menú              │
├────────────────────────────────────────────────────────────┤
│ ┌──────────────┐ ┌──────────────────────┐ ┌──────────────┐ │
│ │ BIBLIOTECA    │ │ PLAYLIST EN AIRE      │ │ CART WALL    │ │
│ │ Búsqueda/filtros│ │ (secuencia + now/next│ │ (páginas de  │ │
│ │ (tags, artista, │ │ + reloj de pista)    │ │ botones)     │ │
│ │ género, década) │ │                      │ │              │ │
│ └──────────────┘ └──────────────────────┘ └──────────────┘ │
│ ┌────────────────────────────────────────────────────────┐ │
│ │ CONSOLA DE AUDIO: EQ, compresor, TalkOver, mix virtual, │ │
│ │ VU meters, ganancia por canal                           │ │
│ └────────────────────────────────────────────────────────┘ │
├────────────────────────────────────────────────────────────┤
│ Barra inferior: Scheduler (próximos eventos), notificaciones│
└────────────────────────────────────────────────────────────┘
```

### 6.3 Interacciones clave

- Doble clic o arrastrar desde biblioteca a playlist/cart wall.
- Atajos de emisión: `Espacio` = play/pause del reproductor principal; `F1–F12` = botones de cart wall; `T` = TalkOver; `S` = stop al final.
- Indicadores de estado claros: color y animación para "en aire", "siguiente", "buffer bajo", "DTMF detectado".
- Formulario de programación tipo calendario (agenda semanal) con drag-and-drop de eventos.
- Toasts de notificación no intrusivos; los errores críticos bloquean con explicación y acción sugerida.

### 6.4 Design system

- Tokens de color, tipografía y espaciado centralizados (CSS custom properties), tema claro y oscuro.
- Componentes: botón de emisión grande, VU meter, waveform mínima por pista, tarjeta de contenido, selector de filtros, tabla de programación.
- Grillas responsivas y anclables; el operador puede guardar layouts (referencia al layout manager de RadioBoss 7.2).

## 7. Arquitectura Limpia

### 7.1 Visión general

Arquitectura hexagonal (puertos y adaptadores) sobre Clean Architecture, aplicada a un cliente desktop Tauri 2 con núcleo de dominio en Rust puro, orquestado por el shell web.

```
┌─────────────────────────────────────────────────────┐
│ PRESENTACIÓN (WebView, React/Svelte/Vue)            │
│  UI + ViewModels (estado derivado, sin lógica)      │
├─────────────────────────────────────────────────────┤
│ APLICACIÓN (Rust, capa de casos de uso)             │
│  Comandos Tauri = puertos de entrada (JSON/TS)      │
├─────────────────────────────────────────────────────┤
│ DOMINIO (Rust, sin dependencias de framework)       │
│  Entidades: Track, Playlist, Schedule, AdBreak,     │
│  Station, User, LogEntry                            │
│  Reglas: rotación, dayparting, anti-repetición,     │
│  mix point, política de tandas                      │
├─────────────────────────────────────────────────────┤
│ INFRAESTRUCTURA (adaptadores)                       │
│  - libmpv (video + audio de precisión)              │
│  - CPAL + Symphonia (pipeline de audio/DSP)         │
│  - SQLite/FTS5 (persistencia)                       │
│  - Shoutcast/Icecast (streaming)                    │
│  - Red (REST/gRPC, SNC/DTMF)                        │
└─────────────────────────────────────────────────────┘
```

### 7.2 Reglas de dependencia

1. El dominio no depende de nada externo (sin Tauri, sin libmpv, sin serde en entidades núcleo, excepto serialización en capa de borde).
2. Los casos de uso dependen solo de puertos (traits) definidos en la capa de aplicación.
3. La UI no llama directamente a la infraestructura: siempre a través de comandos Tauri que ejecutan casos de uso.
4. Los adaptadores son reemplazables: cambiar SQLite por PostgreSQL en la versión servidor no toca dominio ni aplicación.

### 7.3 Entidades del dominio (esquema conceptual)

```
Station { id, name, outputs, timezone }
MediaAsset { id, kind: Audio|Video, file_path, checksum, duration,
             tags, bpm, gain_offset, mix_in, mix_out, intro, outro,
             categories, station_ids }
Playlist { id, name, items: Vec<PlaylistItem>, total_duration }
PlaylistItem { asset_id | stream_url | command, transition, cue_points }
ScheduleEvent { id, station_id, trigger: At(datetime)|DTMF|Recurring,
                actions: Vec<Action>, date_range, dayparting }
AdBreak { id, advertiser, spots: Vec<Spot>, volume_boost, air_window }
PlayoutSession { station_id, started_at, log: Vec<LogEntry> }
```

### 7.4 Motor de playout (crítico)

- Proceso/hilo Rust dedicado con **bucle de tiempo real**: colas de comandos lock-free (ring buffer, `crossbeam`), sin asignaciones en el hot path del audio callback.
- Estados por pista: `Queued → Cued → Playing → FadingOut → Ended`, máquina de estados explícita y auditable.
- El planificador de eventos se evalúa con 30 s de anticipación; todo evento queda materializado en la playlist con marca de origen (manual, scheduler, tanda, satélite) para auditoría.
- Criterio de falla segura: si un recurso programado no existe, el motor salta al siguiente y registra alerta; nunca detiene la emisión.

### 7.5 Persistencia

- SQLite con WAL para la biblioteca local, programación y logs de auditoría; FTS5 para búsqueda.
- Archivos de audio se referencian por ruta + checksum, nunca se copian a una base binaria.
- Sincronización multi-terminal opcional (versión servidor) vía API de sincronización con resolución de conflictos por marca de tiempo.

### 7.6 Decisiones técnicas clave

| Decisión | Elección | Justificación |
|---|---|---|
| Shell de aplicación | Tauri 2 (Rust + WebView del sistema) | Binario de 3–12 MB y 30–50 MB de RAM en reposo frente a los ~80 MB y 200–400 MB de Electron ([dev.to/huzaifa207](https://dev.to/huzaifa207/electron-vs-tauri-how-to-choose-for-your-next-desktop-app-392e)) |
| Motor de video | libmpv | Soporte de códecs y aceleración por hardware; probado en apps de producción con Tauri ([reddit/foss](https://www.reddit.com/r/foss/comments/1xe6acj/i_wanted_a_fast_clean_way_to_watch_my_media/)) |
| DSP de audio | CPAL + Symphonia (fallback de libmpv para crossfade fino y DTMF) | Symphonia con rendimiento a ~10% de FFmpeg ([reddit/oa6539](https://www.reddit.com/r/rust/comments/oa6539/symphonia_v03_purerust_decoders_for_mp3_wav_flac/)); CPAL para I/O PCM ([reddit/x2gd8n](https://www.reddit.com/r/rust/comments/x2gd8n/maybe_this_is_a_dumb_question_but_im_a_bit/)) |
| Persistencia local | SQLite + FTS5 | Embebido, transaccional, búsqueda de texto integral |
| UI | Framework web ligero (React/Svelte) con design system propio | WebView del sistema, ecosistema de componentes |

## 8. Código Limpio

### 8.1 Estándares del código

- **SOLID**: entidades de dominio pequeñas; un caso de uso = un comando Tauri; adaptadores con una sola razón de cambio.
- **Nombres**: dominio en español conceptual (nombres de negocio) pero identificadores de código en inglés; sin abreviaturas no convencionales.
- **Funciones cortas**: máximo ~40 líneas o una pantalla; cyclomatic complexity objetivo < 10.
- **Errores**: tipos de error por capa (`DomainError`, `AppError`, `InfraError`) con `thiserror` en Rust; sin `unwrap()`/`expect()` en código de producción; en TS, `Result`-style o excepciones capturadas en el borde.
- **Comentarios**: solo el "porqué", nunca el "qué"; documentación pública con rustdoc para dominio y con docstrings para comandos Tauri.

### 8.2 Estructura de repositorio (monorepo)

```
repo/
├── crates/
│   ├── domain/            # Entidades y reglas puras (sin deps externas)
│   ├── application/       # Casos de uso + puertos (traits)
│   ├── infrastructure/
│   │   ├── mpv/           # Adaptador libmpv
│   │   ├── audio/         # CPAL + Symphonia pipeline
│   │   ├── db/            # SQLite, migraciones
│   │   ├── streaming/     # Shoutcast/Icecast
│   │   └── net/           # REST/gRPC, SNC/DTMF
│   └── tauri-app/         # Shell: comandos, eventos, IPC
├── ui/                    # App web (React/Svelte) + design system
├── tests/                 # Integración, corpus de audio, smoke de playout
└── docs/                  # Este documento, ADRs, runbooks
```

### 8.3 Prácticas operativas

- Git con ramas cortas, PRs revisadas, Conventional Commits, versión semántica.
- CI/CD: formato (`rustfmt`, `prettier`), linters (`clippy -D warnings`, `eslint`), pruebas unitarias e integración, build multiplataforma, firma de artefactos.
- ADRs (Architecture Decision Records) para toda decisión técnica irreversibles o de alto impacto (por ejemplo, elección de libmpv vs GStreamer).
- Métricas de deuda técnica monitoreadas (clippy warnings, cobertura, duplicación) con presupuesto por release.

### 8.4 Pruebas

| Nivel | Objetivo |
|---|---|
| Unitarias (dominio) | Reglas de rotación, dayparting, cálculo de mix points, política de tandas |
| Unitarias (aplicación) | Casos de uso con puertos simulados (mocks de traits) |
| Integración | Comandos Tauri contra adaptadores reales (SQLite temporal, dispositivos de audio nulos) |
| Humo de audio en CI | Reproducción de corpus de 20 archivos, medición de underruns = 0 |
| E2E | Flujo de programación → emisión → auditoría → reporte |

## 9. Roadmap sugerido

| Fase | Alcance | Criterio de salida |
|---|---|---|
| 1. Núcleo | Shell Tauri + dominio + reproducción simple + biblioteca SQLite | Reproducir una playlist de 10 pistas sin dropouts |
| 2. Emisión | Cart wall, crossfade automático, EQ/compresor, scheduler básico | Emisión asistida de 1 hora sin intervención |
| 3. Automatización | Generador de playlists, rotación por filtros, tandas, dayparting | Emisión automatizada 24 h con auditoría |
| 4. Broadcast | Shoutcast/Icecast, metadatos, TalkOver, logger, DTMF | Retransmisión y pautado comercial completo |
| 5. Red y escala | Multi-emisora, terminales remotas, API REST, reportes | Dos emisoras sincronizadas desde un servidor |

## 10. Referencias

- Dinesat / Hardata — [Página principal](https://www.dinesat.com/home_es.html), [Dinesat 13 Radio](https://www.dinesat.com/radio/es/), [Dinesat Visual Radio](https://www.dinesat.com/visualradio/ar/), [Dinesat IPlay](https://www.dinesat.com/iplay/es/), [Comparativa de productos](https://www.dinesat.com/compare_es.html)
- Hardata (Dinesat 12/13) — [DiarioTI](https://diarioti.com/hardata-lanza-nuevo-software-de-automatizacion-radial/9587)
- Dinesat Pro Radio 11 — [Manual (Scribd, español)](https://es.scribd.com/document/89408294/Hardata-Dinesat-Radio-Quick-Guide-Spanish), [Manual Dinesat Radio (silo.tips)](https://silo.tips/download/hardata-dinesat-radio-radio-automation-manual-de-usuario-version)
- RadioBoss — [Página de producto](https://www.djsoft.net/enu/products/radioboss/radioboss.htm), [Features](https://www.djsoft.net/enu/products/radioboss/features.htm), [Descripción](https://www.djsoft.net/enu/description_radioboss.htm)
- Frameworks desktop — [Comparativa Electron/Tauri/Wails/Flutter/Fyne (dev.to)](https://dev.to/iserter/i-asked-gemini-to-compare-the-best-cross-platform-desktop-app-frameworks-and-here-are-the-results-117l), [Electron vs Tauri (dev.to)](https://dev.to/huzaifa207/electron-vs-tauri-how-to-choose-for-your-next-desktop-app-392e)
- Librerías de audio Rust — [Symphonia](https://www.reddit.com/r/rust/comments/oa6539/symphonia_v03_purerust_decoders_for_mp3_wav_flac/), [Rodio 0.21](https://www.reddit.com/r/rust/comments/1lyobv2/announcing_rodio_021/), [CPAL](https://www.reddit.com/r/rust/comments/x2gd8n/maybe_this_is_a_dumb_question_but_im_a_bit/)
- Casos reales de Tauri + libmpv — [Kinema (reddit/foss)](https://www.reddit.com/r/foss/comments/1xe6acj/i_wanted_a_fast_clean_way_to_watch_my_media/)
