# ADR-009 — Scheduler: evaluación, lookahead y recuperación

- **Estado:** Aceptado
- **Fecha:** 2026-11-15
- **Decisores:** Arquitectura, Programación

## Contexto

El scheduler debe disparar tandas a una hora exacta, respetar reglas de rotación
y separación, y sobrevivir a reinicios. La especificación no define qué pasa si
el operador estaba en manual, si el reloj salta o si la app estuvo cerrada 20
minutos.

## Decisión

- **Evaluación por lookahead**, no por temporizador: cada 500 ms el scheduler
  consulta los eventos cuya ventana `[start, end)` contiene el instante actual
  más una ventana de anticipación configurable (por defecto 2 s).
- **Estados de evento:** `SCHEDULED → ARMED → FIRED → DONE`, más `MISSED` y
  `SKIPPED`. Son persistidos para que un reinicio no repita ni pierda disparos.
- **Idempotencia:** cada disparo tiene una clave `(event_id, scheduled_at_utc_ms)`;
  la tabla de auditoría la usa como `UNIQUE` para evitar dobles emisiones.
- **Modo asistido vs automático:** en `ASSIST` el scheduler arma el evento y
  espera confirmación; en `AUTO` dispara solo. El modo es por estación y se
  persiste.
- **Recuperación tras caída:** al arrancar, se leen los eventos no `DONE` cuya
  ventana sigue abierta y se marcan `ARMED`; los que ya vencieron se marcan
  `MISSED` y se registran en auditoría. La UI ofrece "reanudar" en <10 s.
- **Reloj:** se evalúa en UTC con la zona de la estación (ADR-005). Un salto de
  reloj mayor a 2 s pausa los disparos automáticos y pide confirmación.
- **Separación de reglas:** el cálculo de "qué suena ahora" vive en
  `application`; la persistencia de reglas y la ejecución del disparo, en
  `infra-db` y `playout-engine`. El scheduler nunca habla con CPAL.

## Consecuencias

- Positivas: recuperación explícita y auditable; sin *timer drift* acumulado.
- Negativas: consultar cada 500 ms agrega carga a SQLite (mitigada con índice en
  `(station_id, start_utc_ms)`); la ventana de anticipación debe ajustarse por
  hardware.
- Riesgo: las reglas de rotación (pesos, separación por artista/título) son la
  parte más compleja y se implementan en Fase 3 con tests de propiedad.

## Alternativas consideradas

| Alternativa | Motivo del rechazo |
| --- | --- |
| Un `tokio::sleep` por evento | Se desincroniza con suspensión/reloj del sistema y no sobrevive reinicios. |
| Cron externo del SO | No conoce el estado del playout ni las reglas de rotación. |
| Evaluar solo al hacer clic en "Play" | Pierde la emisión desatendida, que es el objetivo del producto. |
