# ADR-006 — Errores por capa y contrato IPC

- **Estado:** Aceptado
- **Fecha:** 2026-11-15
- **Decisores:** Arquitectura, Backend, Frontend

## Contexto

Una excepción no tipada que cruza de Rust a JavaScript se convierte en un string
sin estructura, y la UI termina mostrando "error: internal error: ...". Además,
el contrato entre el shell y React no tenía esquema: cada cambio de campo rompía
la UI en silencio.

## Decisión

### Errores

- Una enumeración por capa: `DomainError` (invariantes), `AppError` (casos de
  uso, agrupa el dominio), `DbError`/`AudioError`/`AudioError` (adaptadores) y
  `IpcError` (borde del proceso).
- `AppError` deriva `thiserror::Error` y **no** conoce `serde`.
- En el borde, `IpcError { code, message }` mapea cada variante a un código
  estable (`domain`, `not_found`, `invalid_input`, `repository`, `probe`,
  `audio`, `playout`, `configuration`, `internal`). El `message` es para
  diagnóstico; la UI decide por `code`.
- Nunca se devuelve un `panic!` al usuario: `unwrap`/`expect` solo en arranque y
  en tests.

### Contrato IPC

- Los DTO viven en `src-tauri/src/lib.rs`, derivan `Serialize` con
  `#[serde(rename_all = "camelCase")]` y espejan 1:1 los tipos de
  `ui/src/types.ts`.
- Los nombres de comando son `snake_case`; los argumentos se envían en
  camelCase (`assetId`, `categoryId`).
- Todo comando devuelve `Result<T, IpcError>`; los comandos son `async` para no
  bloquear el hilo principal con I/O (SQLite, decodificación).
- Versión del contrato: `app_info.version` expone la versión del paquete. Un
  cambio incompatible de DTO sube la versión menor y se documenta en este ADR.
- Los eventos (en lugar de polling) se agregan en ADR-011; su esquema será
  `{ type, payload }` con el mismo estilo camelCase.

### Superficie actual (v0.1)

| Comando | Argumentos | Devuelve |
| --- | --- | --- |
| `app_info` | — | `AppInfo` |
| `list_categories` | — | `Category[]` |
| `search_library` | `query: LibraryQuery` | `Asset[]` |
| `queue_snapshot` | — | `Asset[]` |
| `playout_snapshot` | — | `PlayoutSnapshot` |
| `playout_command` | `command: "PLAY"\|"PAUSE"\|"STOP"\|"NEXT"\|"STOP_AFTER_CURRENT"` | `PlayoutSnapshot` |
| `enqueue_asset` | `assetId` | `PlayoutSnapshot` |
| `trigger_cart` | `assetId` | `PlayoutSnapshot` |
| `clear_queue` | — | `PlayoutSnapshot` |
| `cart_slots` | `page` | `CartSlot[]` |
| `import_media` | `path`, `categoryId` | `Asset` |
| `toggle_talk_over` | — | `PlayoutSnapshot` |

## Consecuencias

- Positivas: la UI tiene fallback tipado (`normalizeError`) y `code` estable para
  i18n; los errores de infraestructura no filtran detalles al operador.
- Negativas: duplicación entre `AssetDto` y `MediaAsset` al agregar campos; se
  mitiga con un test de contrato (pendiente: generación con `specta`/`ts-rs`).
- Deuda aceptada: `AssetDto` incluye `filePath` que la UI todavía no usa. Se deja
  porque el editor de cue points lo necesita en Fase 1.

## Alternativas consideradas

| Alternativa | Motivo del rechazo |
| --- | --- |
| `Box<dyn Error>` en comandos | No es serializable; la UI recibe un string opaco. |
| Generar tipos con `specta` | Buena opción, pero agrega un paso de build en Fase 0; se evalúa en Fase 1. |
| Eventos en vez de polling | Necesario para medidores; se decide en ADR-011. |
