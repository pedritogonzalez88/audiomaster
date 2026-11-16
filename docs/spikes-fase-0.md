# Spikes de Fase 0 — informe y decisiones

Este documento consolida los resultados de los spikes A–D definidos en
[`plan_implementacion.md §Fase 0`](plan_implementacion.md). Los spikes A y C se
midieron en el entorno de desarrollo; los spikes B y D quedan con decisión
tomada y trabajo pendiente (bloqueantes externos a este repositorio).

Entorno de medición: Windows 11, CPU compartida con navegador y editor, disco
NVMe. Los tiempos son orientativos; el criterio de salida de Fase 1 se
re-mide en el hardware de referencia y en CI.

## Spike A — audio (CPAL + Symphonia)

**Objetivo:** validar que Tauri + CPAL + Symphonia pueden reproducir MP3/FLAC a
un *null sink*, y medir latencia, underruns y CPU con 4 decks.

**Artefacto:** `crates/infrastructure/audio/examples/spike_a.rs`, ejecutable con
`just spike-a <mp3> <flac>`.

**Corpus sintético:** 60 s, 48 kHz, estéreo, generado con ffmpeg (tono 440 Hz +
ruido rosa).

| Medición | Resultado |
| --- | --- |
| Decodificación MP3 (192 kbps) | 104.8 ms para 60 s (RTF 0.002) |
| Decodificación FLAC | 66.3 ms para 60 s (RTF 0.001) |
| Mezcla de 4 decks (60 s de audio) | 7.5 ms de CPU, 0.01 % de un núcleo |
| Null sink, 4 voces, 60 s virtuales | 20.5 ms de pared, 0 underruns |
| Arranque CPAL | 312.1 ms en “Altavoces (Realtek Audio)”, 48 000 Hz / 2 ch |

**Conclusiones:**

1. Symphonia decodifica 60 s de MP3/FLAC en menos de 110 ms: el cuello de
   botella no es la decodificación, sino el consumo de memoria. Se mantiene la
   decisión de decodificar a memoria en Fase 0/1.
2. La mezcla de 4 decks cuesta 7.5 ms por minuto: el callback de audio tiene
   margen enorme. La resolución lineal del resampler es suficiente para el spike;
   en Fase 2 se evalúa `rubato`.
3. El *null sink* es ideal por construcción y no puede underrun. Los underruns
   y la latencia de salida en hardware real se miden en la prueba de soak de
   Fase 1 (10 pistas consecutivas, 60 min, 0 underruns).
4. CPAL necesita un hilo dedicado porque `cpal::Stream: !Send` en Windows; el
   plano de control habla con el callback solo por atómicos (`PlayerShared`).

**Decisión:** ADR-002 aceptado. El motor de audio v1 usa CPAL + Symphonia + DSP
propio, con *null sink* para CI.

## Spike B — video (libmpv)

**Estado:** no medido en este entorno (no hay libmpv instalado). Decisión tomada
en ADR-003; trabajo pendiente.

**Decisión tomada:**

- libmpv como reproductor de video con `hwdec=auto-safe` y sin OSD propio.
- Superficie de salida: ventana nativa separada (Fase 4), no textura embebida en
  el WebView, para poder enviarla a un segundo monitor/HDMI.
- El audio del video se decodifica aparte y se enruta al mismo mezclador que el
  playout, para que *ducking* y talkover apliquen igual; mpv queda con
  `--ao=null`.
- Reloj maestro del motor de playout; el video se esclaviza a él.

**Pendiente (bloqueante para comprometer fecha de Fase 4):**

1. Medir arranque en frío y tamaño de bundle en Windows/macOS/Linux.
2. Verificar `hwdec` en GPU Intel/NVIDIA/AMD y en sesión Wayland vs X11.
3. Definir el protocolo de la ventana separada (posición, fullscreen, cierre).
4. Medir deriva A/V durante 30 min de video con audio por el mezclador.

**Acción:** instalar libmpv de desarrollo en las tres plataformas y ejecutar el
protocolo antes de arrancar Fase 4.

## Spike C — búsqueda (FTS5)

**Objetivo:** medir el P95 de búsqueda con 50.000 pistas sintéticas y ajustar
tokenizer/índices.

**Artefacto:** `crates/infrastructure/db/examples/spike_c.rs`, ejecutable con
`just spike-c [filas]`.

**Corpus:** 50.000 filas, 6 categorías, artistas de 500 nombres, 800 álbumes,
títulos “Adjetivo Sustantivo 00000”. Seed en ~32 s (≈1500 filas/s).

**Resultados (3 rondas, después de `ANALYZE`):**

| Consulta | p50 | p95 |
| --- | --- | --- |
| Lista sin texto (`ORDER BY artist, title`) | 0.4–1.7 ms | 0.5–2.1 ms |
| FTS5 término amplio (un sustantivo común) | 7–12 ms | 8–24 ms |
| FTS5 frase selectiva (dos palabras) | 3–7 ms | 4–10 ms |
| FTS5 con `bm25` (ranking) vs solo orden alfabético | 12.6 vs 7.7 ms | 13.9 vs 8.9 ms |
| Trigram (substring) | 0.08–0.2 ms | 5.3–14 ms |

**Tamaño de la base:** 24.8 MB con FTS5, 32.2 MB agregando el índice trigram
(+7.4 MB, +30 %).

**Plan de la consulta de lista:** `SCAN m USING INDEX
idx_media_assets_live_artist_title` (índice parcial que coincide con el
predicado `is_active = 1 AND deleted_at IS NULL`). Antes del índice parcial y de
`ANALYZE`, el P95 de la lista era 13.2 ms; después, 1.7 ms.

**Conclusiones:**

1. La lista sin texto (caso más común de la UI de biblioteca) queda muy por
   debajo del objetivo con un índice parcial `(artist, title) WHERE activo`.
2. FTS5 con `unicode61` y prefijos cumple <15 ms para consultas selectivas
   (lo que un operador escribe para encontrar una pista). Los términos muy
   comunes son el caso límite (≈15–24 ms) porque `bm25` ordena miles de
   coincidencias.
3. `bm25` cuesta ≈5 ms adicionales frente al orden alfabético. Si la UI ordena
   por relevancia, se paga; si ordena por artista, no.
4. El trigram resuelve substring a 5–14 ms pero agrega un 30 % al tamaño de la
   base. Se deja para un modo “contiene” opcional en Fase 1, no por defecto.

**Decisiones aplicadas al código:**

- `crates/infrastructure/db/src/migrations/0001_baseline.sql`: índice parcial
  `idx_media_assets_live_artist_title` e índice compuesto por categoría.
- `crates/infrastructure/db/src/lib.rs`: la búsqueda sin texto se divide en dos
  sentencias para que SQLite use los índices, y se agrega `Database::analyze()`
  para refrescar las estadísticas tras una importación masiva.
- Tokenizer por defecto: `unicode61` (FTS5). Trigram queda documentado como
  opción.

**Pendiente en Fase 1:** re-medir el P95 en el hardware de referencia; decidir
si la búsqueda por defecto ordena por relevancia (`bm25`) o por artista; si se
mantiene `bm25`, evaluar limitar candidatos o exigir ≥2 caracteres.

## Spike D — licencias y empaquetado

**Estado:** decisión tomada en ADR-007; artefactos en construcción.

**Decisión tomada:**

- RNF-05 se reescribe a ≤80 MB con video o ≤30 MB para el build “radio”.
- Dos instaladores: `audiomaster-core` (sin libmpv) y `audiomaster-video`
  (libmpv/FFmpeg como componente opcional descargable).
- libmpv/FFmpeg en build **LGPL-only** (`--enable-gpl` desactivado, sin
  x264/x265/libfdk-aac) y **enlace dinámico**.
- SBOM CycloneDX + `THIRD_PARTY_LICENSES.md` generados en CI.
- VST/ASIO fuera de v1.0.

**Resultados medidos (build `core`, sin libmpv):**

| Artefacto | Tamaño |
| --- | --- |
| `Audiomaster_0.1.0_x64-setup.exe` (NSIS) | 3.71 MiB |
| `Audiomaster_0.1.0_x64_en-US.msi` (WiX) | 5.16 MiB |
| Ejecutable release (`audiomaster.exe`) | parte del bundle, sin libmpv |

El runtime de WebView2 no se incluye (se descarga en la primera instalación), lo
que mantiene el instalador muy por debajo del objetivo ≤30 MB del build “radio”.

**Pendiente (bloqueante legal, G-17):**

1. Revisión legal de enlace dinámico de libmpv y de códecs patentados
   (AAC/H.264) para Paraguay, y EULA del producto.
2. Medir el tamaño real del instalador `video` con libmpv enlazado dinámicamente
   en las tres plataformas y verificar con `dumpbin`/`ldd`/`otool`.
3. El artefacto de video no se publica hasta cerrar (1).

**Acción:** la revisión legal se gestiona fuera del repositorio; este documento
se actualiza cuando se cierre.

## Criterio de salida de Fase 0

| Criterio | Estado |
| --- | --- |
| 4 spikes con decisión documentada | A y C medidos; B y D con decisión + pendientes externos |
| Build multiplataforma genera instaladores | Windows validado (NSIS 3.71 MiB, MSI 5.16 MiB, sin libmpv); macOS/Linux en CI |
| CI < 10 min | Workflow creado; por validar en la primera ejecución |
| Ningún blocker legal abierto | Abierto (G-17, spike D) |
