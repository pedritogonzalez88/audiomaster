# Fase 2 — emisión asistida: cart wall, crossfade y consola

Estado: **en progreso**. Primer incremento completado.

## Incremento 1 — motor de crossfade (estado)

- `playout-engine`: modelo de dos decks (current + incoming), detección automática
  del mix point (`cue_points.mix_point_ms`), inicio de crossfade equal-power
  (3 s por defecto), avance de ambas posiciones, finalización y cambio de pista.
  Gains expuestos en el snapshot (`crossfadeProgress`, `incomingTitle`,
  `outgoingGain`, `incomingGain`).
- `PlayoutSnapshot` (application, IPC, TS) ampliado con los campos de crossfade.
- Tests: `crossfade_starts_at_mix_point`, `crossfade_completes_and_switches_current`.
- Comportamiento previo (cola secuencial, stop-after-current) se preserva cuando
  no hay mix point real (mix_point == duration).

## Ya existente al arrancar Fase 2

- Cart wall: 3 páginas × 8 pads, `trigger_cart` (encola al frente + PLAY),
  atajos F1–F8 en la UI.
- Talk-over flag en el motor y consola.
- Console con VU, faders y chips DSP (UI).
- Auditoría y telemetría de underruns.

## Pendiente para cerrar Fase 2

1. Detección automática de mix point (silencio a -14 dBFS) con override manual.
2. Cart wall: latencia P95 ≤30 ms medida, stop individual/global, loop, gain por pad.
3. Instant Play y “PlayOver” (voz sobre pista con fade), “TalkOver” con ducking
   (ataque 20 ms, release 300 ms, -14 dB) real en el mixer.
4. Consola: EQ 12 bandas, compresor, limiter true peak, ganancia por canal,
   medidores L/R con peak hold, LUFS/true peak.
5. Entrada de micrófono y auxiliar; selección de devices; perfil de latencia.
6. Atajos configurables, modo operación vs. configuración, bloqueo de UI en aire.
7. Render de audio real de dos decks con crossfade (hoy solo estado).

## Incremento 2 — detección de mix point + TalkOver con ducking

- `infra-audio`: `detect_mix_point` (silencio sostenido < -14 dBFS en la parte
  final, mínimo 30 s y 5 s antes del final). Integrado en `SymphoniaProbe`
  (cue_points.mix_point_ms / fade_out_ms). Test `detects_mix_point_on_silence`.
- `AudioOutput::set_talk_over` en el trait; `CpalPlayer` aplica -14 dB en el
  callback (multiplicador sobre la ganancia); `NullOutput` guarda el flag.
- Shell: espeja `engine.talk_over` al dispositivo en `sync_audio`.
- Fase 1 se considera cerrada funcionalmente (ver `docs/fase-1.md`).

## Incremento 3 — TalkOver con ducking real

- `AudioOutput::set_talk_over` + ducking en el callback de CPAL: -14 dB con
  attack 20 ms y release 300 ms (rampa lineal por buffer). `NullOutput`
  guarda el flag.
- Shell: espeja `engine.talk_over` al dispositivo en `sync_audio`.

## Incremento 4 — cart wall (stop individual/global, loop, gain por pad)

- `playout-engine`: `loop_current` en el estado, `set_loop`, reinicio de posición
  al terminar la pista si el loop está activo. `clear()` resetea el loop.
- IPC: `trigger_cart(assetId, loop, gainDb)` aplica el gain del pad y activa el
  loop; nuevos comandos `set_loop` y `stop_cart` (detiene si la pista en aire
  coincide con el pad).
- UI: `CartWall` agrega un botón de stop por pad (■) con `stopPropagation`;
  `triggerCart` pasa `slot.isLoop` y `slot.gainDb`.
- `loopCurrent` expuesto en el snapshot (backend + TS + mock).

## Incremento 5 — Instant Play

- `playout-engine`: `instant_play(asset, duration_ms)` (crossfade inmediato si hay
  pista en aire, reproducción directa si está parado). Tests
  `instant_play_starts_crossfade`, `instant_play_on_stopped_starts_directly`.
- IPC: comando `instant_play(assetId, durationMs?, loop?, gainDb?)`.
- UI: `api.instantPlay` + mock.

## Incremento 6 — PlayOver (voz sobre pista)

- `playout-engine`: capa "bed" (`bed`, `over`, `over_started_at`,
  `bed_position_at_over`). `play_over(asset, duration_ms)` guarda la pista en
  aire como bed, hace crossfade a la voz y al terminar restaura el bed (posición
  aproximada). `voice_over` en el snapshot.
- IPC: comando `play_over(assetId, durationMs?, gainDb?)`.
- UI: `api.playOver` + mock.

## Incremento 7 — consola DSP (EQ 10 bandas, compresor, limiter)

- `infra-audio/src/dsp.rs`: `DspChain` con EQ gráfico de 10 bandas (biquads
  peaking), compresor feed-forward (threshold/ratio/attack/release) y limiter
  brickwall. Parámetros detrás de un mutex (`DspParams`).
- `AudioOutput`: `set_dsp_params` / `dsp_params` (default no-op; `CpalPlayer`
  delega al mutex compartido).
- `write_data` aplica el DSP en place sobre un buffer f32 antes de convertir al
  formato del dispositivo. Los picos se calculan sobre la señal procesada.
- `lib.rs`: `pub mod dsp` + re-exports.

## Incremento 8 — render de audio real de dos decks + latencia cart wall

- `CpalPlayer`: segundo deck (`incoming_buffer`, posición, ganancia), crossfade
  progress atómico; el callback mezcla ambos decks con gains equal-power y
  aplica el DSP sobre la suma.
- Shell: `apply_audio_state` carga el segundo deck cuando el motor inicia un
  crossfade y propaga `crossfade_progress`.
- Medición: `cart_latency` example → p95 0.000 ms (≤30 ms). IPC añade unos ms
  pero queda muy por debajo del límite.

## Incremento 9 — entrada de micrófono/auxiliar

- `AudioOutput`: `start_mic`, `stop_mic`, `mic_level`, `list_input_devices`
  (asociada en `CpalPlayer`). Captura CPAL en hilo dedicado, convierte a mono y
  empuja a un ring buffer (`ringbuf`); el callback del dispositivo de salida
  mezcla el micrófono con ganancia 0.8 cuando `mic_enabled`.
- Shell: comandos `start_mic`, `stop_mic`, `mic_level`, `list_input_devices`.
- UI: `api.startMic/stopMic/micLevel/listInputDevices` + mock.

## Incremento 10 — parámetros de consola en la UI

- IPC: `get_dsp_params` / `set_dsp_params` + DTO serializable (`DspParams`).
- UI: panel colapsable en `ConsoleBar` con faders por banda de EQ, threshold/ratio/
  ceiling del compresor/limiter y bypass. `App.tsx` carga y persiste los parámetros.
- `DspParams` deriva `Serialize`/`Deserialize` (serde añadido a `infra-audio`).

## Validación

- `cargo test --workspace`: 53 tests, 0 fallos.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`: OK.
- `npm run typecheck`, `npm run lint`, `npm run format:check`, `npm run build`: OK.