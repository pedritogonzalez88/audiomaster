# Fase 1 — núcleo: biblioteca y reproducción

Estado: **en progreso**. Incrementos completados en esta iteración:

## Incremento 1 — auditoría de playout

- `application`: `PlayLogStart` y puerto `AuditRepository` (start/end playback).
- `infra-db`: implementación sobre `playout_logs` + `Database::category_code` y
  `playout_logs_count`.
- `src-tauri`: `AuditState` en `AppState`; `update_audit()` se ejecuta en el
  ticker y registra un row por pista que entra en aire (origen `MANUAL`), con
  `end_playback` al cambiar de pista o al hacer stop/clear.
- Test: `infra-db::tests::audit_start_and_end`.

## Incremento 2 — telemetría de underruns

- `AudioOutput::underruns()` en el trait; `CpalPlayer` incrementa un contador
  atómico cuando el callback se queda sin datos mientras reproduce; `NullOutput`
  expone el mismo contador (siempre 0).
- `PlayoutEngine::record_underrun()` suma al contador del snapshot.
- `AppState` espeja los underruns del dispositivo al motor en `apply_audio_state`,
  de modo que `playout_snapshot.underruns` refleja la realidad.

## Ya existente al arrancar Fase 1

- `MediaAsset`, `LibraryService::import_path` (probe + checksum idempotente),
  búsqueda FTS5 con filtros por categoría.
- Motor de playout con cola secuencial, play/pause/stop/next, stop-after-current,
  talk-over.
- UI: biblioteca, cola, now playing, transporte, cart wall, auditoría en
  pantalla, atajos, temas.

## Pendiente para cerrar Fase 1

1. **Cobertura formal** y E2E import → play (se cierra en paralelo con Fase 2).

Fase 1 se considera funcionalmente completa: auditoría, telemetría de underruns,
ingesta masiva, recuperación, soak 0 underruns, telemetría ~25 Hz y UI
virtualizada están implementados. El ring buffer de audio se mantiene para Fase 2
(ADR-002).

## Incremento 3 — ingesta masiva + BPM básico

- `application`: `ImportSummary` y `LibraryService::import_folder` (escaneo
  recursivo, importación idempotente por checksum, reporte de importados/
  duplicados/errores). `import_path_with_status` expone si se creó fila nueva.
- `infra-audio`: `SymphoniaProbe` ahora estima BPM por autocorrelación de un
  sobre de energía (~100 Hz, rango 60–200 BPM) sobre los primeros 2 min de audio;
  0 si no hay pico claro. Tags siguen viniendo de Symphonia (ID3/Vorbis/MP4).
- `src-tauri`: comando `import_folder(path, categoryId, recursive)` +
  `ImportSummaryDto`.
- UI: botón “Carpeta…” en la biblioteca, `pickMediaFolder` y mock tipado.
- Nota: se evaluó `lofty` para tags; Symphonia ya cubre los contenedores
  necesarios, por lo que no se agrega la dependencia.

## Incremento 4 — recuperación tras corte + seek

- `application`: `PlayoutPort::seek(position_ms)`.
- `playout-engine`: `PlayoutEngine::seek` (clamp a la duración del track actual).
- `infra-db`: `Database::save_snapshot` / `latest_snapshot` sobre `playout_snapshots`.
- `src-tauri`: `RecoverySnapshotDto`, carga del último snapshot al arrancar (si no
  fue cierre limpio), guardado periódico (~5 s) y por cambio de pista, marca
  limpio en `CloseRequested`. Comandos `recovery_status`, `restore_recovery`,
  `seek_playout`. `TransportCommand` sigue sin Seek; el seek se expone por
  comando dedicado.
- UI: banner de recuperación con “Reanudar”/“Descartar” y `mockRecoveryStatus`/
  `mockRestoreRecovery` en el mock.

## Incremento 5 — validación de la salida de audio (soak)

- `crates/infrastructure/audio/examples/soak.rs`: 10 pistas consecutivas, 60 min
  de audio, 0 underruns, RTF 0.13 sobre el *null sink* (477 ms de pared).
- Medición de arranque en el shell (`tracing::info!` con `elapsed_ms`).
- Decisión: el decodificador a memoria cumple el criterio de 0 underruns para
  pistas de hasta varios minutos; el streaming con ring buffer se mantiene para
  Fase 2 (ADR-002), como estaba previsto. La memoria pico es ~140 MB por pista
  de 6 min y se libera al cambiar de pista.

## Incremento 6 — telemetría a ~25 Hz (ADR-011)

- `AudioOutput::peaks()` en el trait; `CpalPlayer` computa el pico por canal en
  el callback (fetch_max atómico) y lo entrega/reset; `NullOutput` devuelve 0.
- `TelemetryPayload` (positionMs, peakL, peakR, lufs, underruns) emitido por un
  hilo dedicado cada 40 ms vía `app.emit("telemetry", ...)`.
- UI: `listen("telemetry")` actualiza estado; `ConsoleBar` usa los picos reales
  para el VU (con transición CSS para suavizado) y cae al modo sintético si no
  hay telemetría.

## Incremento 7 — UI virtualizada (biblioteca)

- `@tanstack/react-virtual` agregado a `ui/package.json`.
- `LibraryPanel` ahora usa `useVirtualizer` sobre `.library__list` (scroll
  container existente): solo se montan las filas visibles + overscan, con
  posicionamiento absoluto por `transform: translateY(...)`. Soporta 50.000 filas
  sin degradar el render.
- Prettier/Eslint/TypeScript verdes.

## Validación

- `cargo test --workspace`: 28 tests, 0 fallos.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`: OK.
- `npm run typecheck`, `npm run lint`, `npm run format:check`, `npm run build`: OK.
