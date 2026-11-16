# Fase 0 — fundaciones y reducción de riesgo

Estado: **entregables técnicos completos; criterio de salida condicionado a dos
pendientes externos** (build en macOS/Linux y revisión legal de G-17). Ver
[`docs/spikes-fase-0.md`](spikes-fase-0.md) y [`docs/adr/README.md`](adr/README.md).

## Entregables (checklist)

- [x] `git init`, estructura de monorepo.
- [x] Workspace Rust con crates y reglas de dependencia; lints (`clippy -D
      warnings`), `rustfmt` (`rustfmt.toml`).
- [x] CI base: Windows/macOS/Ubuntu, caché, `nextest`, `clippy`, `rustfmt`,
      `cargo-deny`, `cargo-audit`; frontend `tsc`, ESLint, Prettier, Vitest
      (`.github/workflows/ci.yml`).
- [x] ADR-001 a ADR-012 en `docs/adr/` + índice con reglas de dependencia.
- [x] Contratos IPC: DTOs versionados, códigos de error (`IpcError`), esquema
      de comandos en ADR-006. Generación automática (ts-rs/specta) queda para
      Fase 1; hoy la paridad se mantiene a mano con tests.
- [x] Migración base SQLite + runner + seed de categorías/emisora/cart wall
      (`0001_baseline.sql`, 6 categorías, `Radio Master FM`, 3 páginas × 8 pads).
- [x] Spike A medido y documentado (`docs/spikes-fase-0.md`).
- [ ] Spike B medido (libmpv no instalado en este entorno).
- [x] Spike C medido y documentado; tokenizer/índices ajustados.
- [ ] Spike D medido (revisión legal abierta).
- [x] Design system: tokens del prototipo, temas claro/oscuro/alto contraste,
      componentes base accesibles (`ui/src/design-system/tokens.css`,
      `ui/src/styles.css`).
- [x] Shell de la app con layout del prototipo, navegación y atajos
      (`ui/src/App.tsx` y componentes).

## Artefactos creados

- `Cargo.toml`, `rustfmt.toml`, `deny.toml`, `justfile`, `LICENSE`,
  `README.md`, `CONTRIBUTING.md`.
- `src-tauri/`: `Cargo.toml`, `build.rs`, `tauri.conf.json`,
  `capabilities/default.json`, iconos, `src/lib.rs`, `src/main.rs`.
- `docs/adr/0001..0012-*.md` y `docs/adr/README.md`.
- `crates/infrastructure/audio/examples/spike_a.rs`.
- `crates/infrastructure/db/examples/spike_c.rs`.

## Validación técnica

- `cargo fmt --all --check`: OK.
- `cargo clippy --workspace --all-targets -- -D warnings`: OK.
- `cargo test --workspace`: 27 tests, 0 fallos (domain 5, application 2,
  playout-engine 6, infra-db 5, infra-audio 9).
- `npm run typecheck`, `npm run lint`, `npm run format:check`, `npm run test`
  (9 tests), `npm run build` (253 kB JS): OK.

## Pendientes para cerrar el criterio de salida

1. **Build multiplataforma:** Windows validado (NSIS 3.71 MiB, MSI 5.16 MiB, sin
   libmpv); macOS y Linux se validan en CI.
2. **CI < 10 min:** por validar en la primera ejecución del workflow.
3. **Blocker legal (G-17):** revisión de licencias de libmpv/FFmpeg. Afecta
   únicamente el artefacto de video (Fase 4); no bloquea Fase 1.

## Recomendación

Fase 1 (biblioteca + reproducción real) puede arrancar mientras se resuelve (3):
no depende de video ni del empaquetado LGPL. Los puntos (1) y (2) se cierran
con la primera ejecución de CI sin cambios de código. Antes de arrancar Fase 1,
confirmar con el usuario si se acepta avanzar con el blocker legal abierto.
