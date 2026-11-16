# Fase 3 — automatización: scheduler, rotación y tráfico

Estado: **en progreso** (primer incremento).

## Incremento 1 — modelo y runtime del scheduler

- `domain/src/scheduler.rs`: `ScheduleEventKind` (At/Recurring/Dtmf/Manual/Api),
  `ScheduleActionKind`, `ScheduleStatus` (Scheduled/Armed/Fired/Done/Missed/Skipped),
  `ScheduleEvent`, `Recurrence`, `Daypart` con validaciones y tests.
- `application/src/scheduler.rs`: `SchedulerRuntime` con `evaluate(now_ms)` que arma
  eventos en la ventana de lookahead (30 s), dispara los vencidos, marca
  `Missed` y permite `mark_done`. Tests de firing, arming, missed, idempotencia
  y done.

## Pendiente para cerrar Fase 3

1. Persistencia: repositorio SQLite para schedule_events/actions + runner.
2. Motor de rotación: filtros, cuotas, pesos, separación, anti-repetición.
3. Generador de playlists por plantilla.
4. Tráfico: campañas, spots, vigencia, compilación de tandas, `ad_airings`.
5. HTH, modo AUTO/ASISTIDO, recuperación, UI de calendario, reportes.

## Validación

- `cargo test --workspace`: 62 tests, 0 fallos.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`: OK.
- `npm run typecheck`, `npm run lint`, `npm run format:check`, `npm run build`: OK.