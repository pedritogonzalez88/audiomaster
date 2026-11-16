# ADR-005 — Modelo de tiempo: UTC epoch ms y zonas

- **Estado:** Aceptado
- **Fecha:** 2026-11-15
- **Decisores:** Arquitectura, Scheduler

## Contexto

La programación de tandas y la auditoría cruzan husos horarios, cambios de
horario de verano y ajustes manuales del reloj del operador. Las
especificaciones mezclan horas locales ("15:00"), duraciones y marcas de
auditoría sin definir el tipo.

## Decisión

- **Persistencia:** toda marca de tiempo se guarda como `INTEGER` de
  milisegundos desde la época Unix **en UTC** (`at_utc_ms`, `created_at`,
  `last_played_at`). Nunca se guarda hora local.
- **Duración:** milisegundos enteros (`u32`/`u64`), sin punto flotante. Las
  posiciones de reproducción son `u64` para no desbordar en sesiones largas.
- **Zona horaria:** la estación define su `timezone` (IANA, por defecto
  `America/Asuncion`). El scheduler evalúa en la zona de la estación convirtiendo
  el instante UTC con `chrono-tz` (se agrega en Fase 1 junto al scheduler).
- **Reloj inyectable:** `application::Clock` con `now_utc_ms()`. Producción usa
  `SystemClock`; los tests usan un reloj manual (`AtomicI64`) para que la máquina
  de estados sea determinista.
- **Cambios de horario:** el scheduler detecta huecos y solapamientos DST y
  emite un evento de auditoría en vez de "arreglarlos" silenciosamente.
- **NTP:** el shell compara el reloj del sistema con la hora del logger; si la
  deriva supera 2 s se avisa en la barra superior.

## Consecuencias

- Positivas: comparaciones y ordenamientos triviales; la auditoría es
  independiente del huso del operador; los tests no dependen del reloj real.
- Negativas: toda presentación requiere conversión explícita a la zona de la
  estación; los reportes deben declarar su zona.
- Riesgo: `chrono-tz` agrega peso y una base de datos de zonas que debe
  actualizarse con el ciclo de releases (los cambios DST cambian por ley).

## Alternativas consideradas

| Alternativa | Motivo del rechazo |
| --- | --- |
| Hora local como texto | Ordenamiento y cálculo de diferencias frágiles. |
| Segundos desde epoch | Insuficiente resolución para cue points y telemetría. |
| `chrono::DateTime` en la DB | Formato dependiente del serializador; difícil de indexar. |

## Verificación

`playout-engine` usa un `ManualClock` en todos sus tests; los DTO IPC exponen
`positionMs`/`durationMs` como enteros y la UI formatea con `Intl` en `es-PY`.
