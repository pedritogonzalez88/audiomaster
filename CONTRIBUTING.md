# Contribución

## Alcance

Audiomaster está en Fase 0: el trabajo activo es la fundación (arquitectura,
contratos IPC, spikes). Antes de abrir una contribución, lee
[`docs/plan_implementacion.md`](docs/plan_implementacion.md) y los ADRs en
[`docs/adr/`](docs/adr/README.md). Cualquier cambio que contradiga un ADR
requiere un ADR nuevo que lo supersedan, no una edición silenciosa.

## Requisitos de entorno

Ver [`README.md`](README.md#requisitos). En Linux instala las dependencias de
Tauri con `apt` antes de compilar.

## Flujo

1. Crea una rama descriptiva (`feat/`, `fix/`, `docs/`, `refactor/`).
2. Haz cambios pequeños y testeables. El dominio y la aplicación no conocen
   Tauri, SQLite, CPAL ni libmpv: respeta las reglas de dependencia del
   [`docs/adr/README.md`](docs/adr/README.md).
3. Ejecuta `just check` (o los comandos manuales) antes de abrir el PR:
   - `cargo fmt --all --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace` (o `cargo nextest run --workspace`)
   - `npm run typecheck`, `npm run lint`, `npm run format:check`, `npm run test`,
     `npm run build`
4. Escribe tests para el comportamiento nuevo. Objetivos de cobertura: dominio
   ≥85 %, aplicación ≥75 %.
5. Actualiza la documentación afectada (ADR, `docs/fase-0.md`, runbook).

## Estilo de commits

Conventional Commits en español o inglés, sin mezclar idiomas en un mismo commit:
`feat(playout): ...`, `fix(db): ...`, `docs(adr): ...`, `refactor(audio): ...`,
`test(domain): ...`, `chore(ci): ...`.

## Reglas de código

- **Sin `unwrap()`/`expect()` en código de producción** fuera del arranque y los
  tests. Los errores cruzan la frontera como `IpcError { code, message }`.
- **Sin `print!`/`println!`** en producción. Usar `tracing`.
- **Sin `unsafe`** salvo justificación documentada y revisión.
- **Sin secretos en el repositorio**, ni en SQLite en texto plano: van al
  keyring del SO (ADR-008).
- Los DTO IPC viven en `src-tauri/src/lib.rs` y deben espejar
  `ui/src/types.ts` exactamente (camelCase). Un cambio incompatible sube la
  versión del contrato y se documenta en ADR-006.
- `cargo fmt` y `rustfmt.toml` son la única fuente de verdad del formato Rust.

## Lints de workspace

`Cargo.toml` raíz define los lints y cada crate usa `[lints] workspace = true`:
`unsafe_code = warn`, `clippy::all = warn`, `dbg_macro = deny`,
`print_stdout`/`print_stderr = warn`. El CI falla con `-D warnings`.

## ADRs

Los ADRs se numeran una sola vez y no se reescriben. Para cambiar una decisión,
crea un ADR nuevo que la supersede y marca el anterior como
`Reemplazado por ADR-XXX`. El índice está en [`docs/adr/README.md`](docs/adr/README.md).

## CI

Ver [`.github/workflows/ci.yml`](.github/workflows/ci.yml): Ubuntu/macOS/Windows,
`nextest`, `clippy`, `rustfmt`, `cargo-deny`, `cargo-audit` y el frontend
(`tsc`, ESLint, Prettier, Vitest, build).

## Licencia

Al contribuir aceptas que tu aporte se licencie bajo los términos del proyecto.
Ver [`LICENSE`](LICENSE).
