# Audiomaster

Estudio de playout y automatización para radio y TV. Aplicación de escritorio
multiplataforma (Windows, macOS, Linux) construida con **Tauri 2 + Rust** y una
UI en **React 19 + TypeScript**.

Estado: **Fase 3 en progreso** — Fase 0 y Fase 1 completas, Fase 2 con 9 incrementos
(crossfade, mix point, cart wall, TalkOver, consola DSP, mic/aux). Ver
[`docs/fase-2.md`](docs/fase-2.md) y [`docs/fase-3.md`](docs/fase-3.md).

## Qué incluye hoy

- Workspace Rust con reglas de dependencia entre capas (`domain` → `application`
  → `playout-engine` / `infra-*` → `src-tauri`).
- Motor de playout determinista: cola, play/pause/stop, next, stop-after-current,
  talk-over, con reloj inyectable y tests de estado.
- Audio con CPAL 0.15 + Symphonia 0.5: decodificación a memoria, salida en hilo
  dedicado y *null sink* para tests/CI.
- Biblioteca SQLite (rusqlite `bundled`, FTS5) con migración base, triggers de
  búsqueda, repositorio y seed de emisora/categorías/cart wall.
- Shell Tauri con IPC tipado (`IpcError { code, message }`, DTO camelCase) y
  ticker de control cada 100 ms.
- UI operativa de Fase 0: transporte, now playing, cola, biblioteca, cart wall de
  3 páginas × 8 pads, auditoría en pantalla, atajos y temas claro/oscuro/alto
  contraste. Si la app corre en el navegador sin Tauri, usa un mock tipado.
- 27 tests Rust + 9 tests de frontend; `clippy -D warnings`, `rustfmt`, ESLint,
  Prettier y `tsc --strict`.

## Requisitos

| Herramienta | Versión |
| --- | --- |
| Rust | stable (ver `rust-toolchain.toml`) |
| Node.js | 22 LTS o superior |
| Windows | Visual Studio Build Tools 2022 con “Desktop development with C++” |
| macOS | Xcode Command Line Tools |
| Linux | `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`, `libssl-dev` |

## Puesta en marcha

```bash
npm install
npm run tauri dev      # ventana de escritorio con hot reload de la UI
npm run tauri build    # instalador de la plataforma
```

Solo la UI, sin Tauri (usa el mock):

```bash
npm run dev            # http://localhost:5173
```

La base de datos se crea en el directorio de datos de la app
(`%APPDATA%\com.audiomaster.studio\audiomaster.db` en Windows). No se guarda
junto al ejecutable.

## Comandos

| Comando | Qué hace |
| --- | --- |
| `just check` | fmt + clippy + tests Rust + tests UI |
| `just test` | `cargo nextest` + Vitest |
| `just lint` | `clippy -D warnings`, `cargo fmt --check`, ESLint, Prettier |
| `just dev` / `just build` | Tauri en desarrollo / instalador |
| `just spike-a <mp3> <flac>` | decodificación + mezcla de 4 decks + arranque CPAL |
| `just spike-c [filas]` | búsqueda FTS5/trigram con 50.000 filas sintéticas |

Sin `just`, los equivalentes son `npm run typecheck`, `npm run lint`,
`npm run format:check`, `npm run test`, `npm run build` y
`cargo nextest run --workspace` / `cargo test --workspace`.

## Estructura

```
crates/
  domain/              entidades, value objects, curvas de crossfade
  application/         casos de uso y puertos (sin Tauri/SQLite/CPAL)
  playout-engine/      máquina de estados del playout
  infrastructure/
    db/                SQLite + FTS5 + migraciones
    audio/             probe, decoder, CPAL, null sink
src-tauri/             shell de escritorio, IPC y wiring
ui/                    React + TypeScript + Vite
docs/                  plan, ADRs, spikes, runbooks
tests/                 corpus y espacio para E2E/integración
```

Regla de oro: `application` no importa Tauri, SQLite, CPAL ni libmpv. El único
crate que conoce todas las implementaciones es `src-tauri`.

## Documentación

- [`docs/plan_implementacion.md`](docs/plan_implementacion.md) — plan por fases.
- [`docs/fase-0.md`](docs/fase-0.md) — entregables y criterios de salida.
- [`docs/spikes-fase-0.md`](docs/spikes-fase-0.md) — resultados medidos y
  decisiones de los spikes A–D.
- [`docs/adr/README.md`](docs/adr/README.md) — índice de decisiones (ADR-001 a
  ADR-012) y reglas de dependencia.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — flujo de trabajo y convenciones.

## Limitaciones conocidas de esta fase

- El audio se decodifica completo en memoria (~23 MB por minuto estéreo a 48 kHz).
  Fase 2 lo reemplaza por streaming con ring buffer.
- El cart wall encola al frente; el disparo instantáneo con segundo deck llega en
  Fase 3.
- Búsqueda: FTS5 con tokenizer `unicode61` y prefijos; el modo substring con
  trigram queda evaluado pero no habilitado (ver spike C).
- Video (libmpv) y empaquetado LGPL: decisión tomada, implementación en Fase 4 y
  revisión legal abierta.

## Licencia

Propietario. Ver [`LICENSE`](LICENSE).
