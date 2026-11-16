# Registro de decisiones de arquitectura (ADR)

Cada ADR documenta una decisión que es costosa de revertir. Se numeran una sola
vez y no se reescriben: si una decisión cambia, se agrega un ADR nuevo que la
supersede y se actualiza el estado del anterior a `Reemplazado por ADR-0XX`.

Estados posibles: `Propuesto`, `Aceptado`, `Rechazado`, `Reemplazado`.

| ADR | Tema | Estado |
| --- | --- | --- |
| [ADR-001](0001-ui-react-typescript.md) | Framework UI: React 19 + TypeScript | Aceptado |
| [ADR-002](0002-motor-audio-cpal-symphonia.md) | Motor de audio: CPAL + Symphonia + DSP propio | Aceptado |
| [ADR-003](0003-video-libmpv.md) | Video: libmpv, superficie y audio | Aceptado (spike B pendiente) |
| [ADR-004](0004-persistencia-sqlite.md) | SQLite bundled + FTS5 + escritor único | Aceptado |
| [ADR-005](0005-modelo-de-tiempo.md) | Modelo de tiempo: UTC epoch ms y zonas | Aceptado |
| [ADR-006](0006-errores-y-contrato-ipc.md) | Errores por capa y contrato IPC | Aceptado |
| [ADR-007](0007-licencias-y-empaquetado.md) | Licencias y empaquetado de libmpv/FFmpeg | Aceptado con revisión legal abierta |
| [ADR-008](0008-secretos-keyring.md) | Secretos con keyring del SO | Aceptado |
| [ADR-009](0009-scheduler-evaluacion.md) | Scheduler: evaluación, lookahead y recuperación | Aceptado |
| [ADR-010](0010-sincronizacion-multiterminal.md) | Sync v1: escritor único, CRDT diferido | Aceptado |
| [ADR-011](0011-telemetria-ui.md) | Telemetría UI a 20-30 Hz | Aceptado |
| [ADR-012](0012-plugins-fuera-de-v1.md) | Plugins fuera de v1.0 | Aceptado |

## Reglas de dependencia entre crates

```
domain            (sin I/O, sin framework)
  ↑
application       (casos de uso, define puertos)
  ↑
playout-engine    (máquina de estados determinista)
infra-db          (SQLite)
infra-audio       (CPAL/Symphonia)
  ↑
src-tauri         (shell, IPC, wiring)
```

`domain` no importa nada de los demás crates. `application` no importa Tauri,
SQLite, CPAL ni libmpv. `src-tauri` es el único lugar donde se conocen todas las
implementaciones a la vez. Esto se verifica con `cargo-deny` (prohibición de
dependencias cruzadas) y con revisión de PR.

Ver también: [`docs/plan_implementacion.md`](../plan_implementacion.md) y el
informe de spikes en [`docs/spikes-fase-0.md`](../spikes-fase-0.md).
