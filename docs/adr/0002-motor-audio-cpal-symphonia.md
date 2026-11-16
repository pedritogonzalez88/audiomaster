# ADR-002 — Motor de audio: CPAL + Symphonia + DSP propio

- **Estado:** Aceptado
- **Fecha:** 2026-11-15
- **Decisores:** Arquitectura, Audio

## Contexto

El playout requiere dos decks con crossfade, cart wall con disparo casi
inmediato, talkover con *ducking* y medición de loudness EBU R128. Necesitamos
control del reloj de audio, del buffer y del mezclador; un reproductor de alto
nivel (rodio, bass, gstreamer) no expone el punto de mezcla necesario.

## Decisión

- **CPAL 0.15** para salida de audio. Se usa el dispositivo *default* y su
  formato nativo (`f32`, `i16`, `u16`, `i32`), convirtiendo por muestra.
- **Symphonia 0.5** (feature `all`) para decodificar MP3/FLAC/WAV/OGG/AAC.
- **DSP propio** en `domain::crossfade` (curvas equal-power y lineal, conversión
  dB ↔ lineal) para que las transiciones sean testeables sin hardware.
- **`cpal::Stream` no es `Send`** en Windows: el stream se crea y se mantiene
  vivo en un hilo dedicado (`audiomaster-cpal`). El plano de control habla con
  el callback solo por atómicos (`PlayerShared`) y el hilo se apaga cerrando un
  canal `crossbeam-channel`.
- **Fase 1 decodifica a memoria**; la sustitución por *streaming* con ring
  buffer (`ringbuf`) queda localizada en `CpalPlayer::load` / `decode_to`.
- **Null sink** (`NullOutput`) para CI y tests deterministas; nunca requiere
  hardware.
- La lógica de transporte (cola, pausa, stop-after-current) vive en
  `playout-engine`, no en el adaptador de audio.

## Consecuencias

- Positivas: control total del buffer y de la mezcla; tests de crossfade sin
  dispositivo; dependencias con licencia permisiva (MIT/Apache-2.0).
- Negativas: un track de 5 min estéreo a 48 kHz ocupa ~110 MB como `f32` en
  memoria. **Aceptado solo para Fase 0/1**; Fase 2 introduce streaming y el
  requisito de cero *underruns* en 60 min.
- La resolución de muestras es lineal (`resample_linear`), suficiente para el
  spike pero no para calidad de emisión: Fase 2 evalúa `rubato`.
- Riesgo: WASAPI exclusivo y ASIO quedan fuera de v1.0 (ver ADR-007).

## Alternativas consideradas

| Alternativa | Motivo del rechazo |
| --- | --- |
| rodio | No expone punto de mezcla por deck ni reloj de muestras. |
| libmpv para audio | Latencia de arranque y control de seek insuficientes para cart wall. |
| GStreamer | Pipeline potente pero instalador y licencias mucho más pesados. |
| ASIO directo | Licencia y complejidad; se evalúa como plugin opcional. |

## Verificación

Spike A: `docs/spikes-fase-0.md` (latencia de arranque, underruns y CPU con un
sink nulo y con CPAL real).
