# ADR-004 — Persistencia: SQLite bundled + FTS5 + escritor único

- **Estado:** Aceptado
- **Fecha:** 2026-11-15
- **Decisores:** Arquitectura, Datos

## Contexto

El sistema es una aplicación de escritorio monousuario por estación, con
necesidad de búsqueda de texto completo sobre 50.000 pistas, integridad
referencial (categorías, tandas, programación) y funcionamiento sin servidor. Un
motor cliente-servidor (Postgres) complica la instalación; un ORM con
migraciones automáticas oculta el SQL que necesitamos controlar para FTS5 y
rendimiento.

## Decisión

- **SQLite embebido** vía `rusqlite 0.32` con la feature `bundled` (compila
  SQLite con el crate; no depende de la versión del sistema).
- **FTS5** con una tabla virtual `media_assets_fts(title, artist, album)` y
  triggers `AFTER INSERT/UPDATE/DELETE` que la mantienen sincronizada.
- **Escritor único:** una conexión detrás de `parking_lot::Mutex` por proceso.
  Los lectores de larga duración (futuro servidor) usan una conexión aparte.
- **Migraciones versionadas** en `crates/infrastructure/db/src/migrations/`,
  aplicadas en una transacción y registradas en `schema_migrations` con checksum
  FNV-1a. Los archivos ya publicados no se editan; los cambios van en `000N_*`.
- **WAL** con `synchronous=NORMAL`, `busy_timeout=5000` y `foreign_keys=ON`.
- La base vive en el directorio de datos de la app (`app_data_dir()` de Tauri),
  nunca junto al ejecutable.
- Los repositorios se exponen como **puertos** (`application::MediaRepository`);
  `infra-db` es la única implementación que conoce `rusqlite`.

## Consecuencias

- Positivas: instalación sin servidor, backup por copia de archivo + WAL,
  búsqueda <15 ms con los índices correctos, SQL explícito y auditable.
- Negativas: concurrencia de escritura limitada a un proceso. Esto choca con
  RNF-08 (10+ terminales) y se resuelve con ADR-010.
- Riesgo: FTS5 no viene en algunos builds de SQLite de sistema; la feature
  `bundled` lo elimina pero agrega ~1 MB al binario.

## Alternativas consideradas

| Alternativa | Motivo del rechazo |
| --- | --- |
| PostgreSQL | Requiere servidor; sobredimensionado para una estación. |
| DuckDB | Excelente analítica, menos maduro para OLTP con escrituras concurrentes. |
| LMDB / sled | Sin SQL ni FTS; habría que escribir el motor de consulta. |
| SQLite del sistema | Versiones y flags inconsistentes entre Windows/macOS/Linux. |

## Verificación

`crates/infrastructure/db/src/lib.rs` incluye tests de migración, FTS sincronizado
en update/delete, filtros por categoría y sanitización de la consulta FTS.
Spike C mide P95 con 50.000 filas.
