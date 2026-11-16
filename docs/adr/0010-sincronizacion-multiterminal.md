# ADR-010 — Sincronización v1: escritor único y concurrencia optimista (CRDT diferido)

- **Estado:** Aceptado
- **Fecha:** 2026-11-15
- **Decisores:** Arquitectura, Producto

## Contexto

RNF-08 pide soporte para "10+ terminales" con edición concurrente. SQLite tiene
un solo escritor por base y no es una base distribuida. Interpretar RNF-08 como
"CRDT en todas las tablas" implicaría reescribir el modelo de datos y renunciar a
las restricciones relacionales que dan integridad a la programación y a la
facturación de tandas.

## Decisión

- **v1.0: arquitectura de escritor único.** Una estación tiene un proceso
  "servidor" (el escritorio principal o un servicio local) que es el único que
  escribe en SQLite.
- Las terminales adicionales son **clientes** que consumen una API local/HTTP y
  envían comandos (encolar, disparar cart, cambiar modo). La UI del operador y el
  motor de playout viven en el nodo servidor.
- **Concurrencia optimista:** cada fila editable lleva `updated_at` y
  `revision`; un cliente envía `expected_revision` y recibe `409 CONFLICT` si
  otro la modificó. Los conflictos de programación se resuelven por último
  escritor con auditoría, no fusionando automáticamente.
- **CRDT se difiere a v2** y solo para edición de programación (no para playout
  ni auditoría). Requisito de entrada: tener el modelo de eventos estable.
- La telemetría se distribuye por *push* del servidor a los clientes; los
  comandos de transporte son idempotentes y llevan un `command_id`.

## Consecuencias

- Positivas: integridad referencial intacta, un solo lugar donde auditar, sin
  resolución de conflictos en el camino crítico de emisión.
- Negativas: el nodo servidor es un punto único de falla; si se cae, la emisión
  se detiene. Se mitiga con snapshot de estado (`playout_snapshots`) y
  recuperación rápida (ADR-009).
- Deuda: el protocolo cliente-servidor se especifica en Fase 5; en Fase 0-1 solo
  existe el proceso único con IPC de Tauri.

## Alternativas consideradas

| Alternativa | Motivo del rechazo |
| --- | --- |
| CRDT en v1.0 | Semanas de diseño, pierde FK y complica la auditoría fiscal. |
| Postgres central | Requiere servidor y administración que el cliente no tiene. |
| SQLite replicado (Litestream/rqlite) | Replicación de lectura, no concurrencia de edición. |
| Bloqueo pesimista total | Un operador bloquea a los demás durante toda la edición. |
