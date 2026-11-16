# ADR-011 — Telemetría de UI a 20-30 Hz y agregación

- **Estado:** Aceptado
- **Fecha:** 2026-11-15
- **Decisores:** Arquitectura, Frontend

## Contexto

Los medidores VU, el reloj de cuenta regresiva y la posición del deck necesitan
actualizarse de forma fluida (20-30 Hz). Emitir un evento IPC por muestra de
audio satura el puente WebView↔Rust y hace que el operador vea *jank* justo
cuando necesita reaccionar. Polling a 2 Hz (lo que hace el shell hoy) es
suficiente para estado y cola, pero no para medidores.

## Decisión

- **Estado y cola: polling a 2 Hz** (`playout_snapshot`, `queue_snapshot`).
  Barato, idempotente y suficiente para textos.
- **Medidores y posición: un solo evento agregado a 25 Hz**, emitido por el
  hilo de control solo cuando hay un cliente suscrito:
  ```json
  { "type": "telemetry", "payload": { "positionMs": 102340, "peakL": -12.4,
    "peakR": -11.9, "lufs": -23.1, "underruns": 0 } }
  ```
- El evento se emite como **un frame fijo** (no un evento por canal) y se
  descarta si el WebView está oculto (`document.visibilityState`).
- La UI **interpola** entre frames para animar el medidor; nunca escribe estado
  de React a 25 Hz: el medidor escribe directamente sobre `style.width` de un
  `ref` para evitar re-render del árbol.
- El reloj de pared se calcula en el frontend; no se envía por IPC.
- Los peaks de audio provienen del callback de CPAL por un `AtomicU32` (f32 en
  bits), no de una cola de mensajes: el callback nunca bloquea.

## Consecuencias

- Positivas: carga predecible, sin re-render del árbol completo, medidores
  suaves, y el estado crítico sigue siendo consultable por polling si se pierde
  un evento.
- Negativas: dos mecanismos (polling + eventos) que documentar y testear.
- Riesgo: un `setState` accidental por frame degradaría la UI; se cubre con
  revisión y con el modo `prefers-reduced-motion`.

## Alternativas consideradas

| Alternativa | Motivo del rechazo |
| --- | --- |
| Polling a 25 Hz | 25 IPC/s × N comandos satura el puente y el GC. |
| Un evento por canal/medidor | Muchos frames pequeños; peor latencia agregada. |
| WebAudio en el frontend | El audio vive en Rust; duplicaría el camino de datos. |
| Canvas con `requestAnimationFrame` sobre SharedArrayBuffer | Sin soporte estable en WebView2/WKWebView. |
