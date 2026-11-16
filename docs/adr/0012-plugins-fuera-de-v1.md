# ADR-012 — Plugins fuera de v1.0 y aislamiento futuro

- **Estado:** Aceptado
- **Fecha:** 2026-11-15
- **Decisores:** Arquitectura, Producto, Seguridad

## Contexto

Las especificaciones mencionan extensibilidad (efectos, fuentes de audio,
integraciones). Un sistema de plugins implica cargar código de terceros en el
proceso que está **en aire**, con acceso al audio y a la base de datos. Eso es
incompatible con un criterio de "cero blockers legales" y con la estabilidad que
exige la emisión.

## Decisión

- **v1.0 no tiene plugins.** La extensibilidad se limita a:
  - *hot folders* de ingesta y formatos de metadata,
  - plantillas de programación y rotación,
  - salidas de streaming (Icecast/Shoutcast) por configuración.
- Las interfaces internas (`MediaProbe`, `PlayoutPort`, `AudioOutput`) se diseñan
  como puertos para que un plugin futuro sea una implementación, no un parche.
- **v2 (si se aprueba):** plugins en **proceso separado** con un protocolo IPC
  versionado, sin acceso directo a SQLite ni al dispositivo de audio. El host
  aplica cuotas y *sandbox* del SO. Un plugin no puede bloquear el hilo de audio.
- El *feature flag* de plugins no se agrega hasta que exista el protocolo y el
  modelo de permisos; no se aceptan aportes de plugins en v1.0.

## Consecuencias

- Positivas: superficie de ataque mínima, sin carga de código de terceros en
  emisión, instalador más simple y auditable.
- Negativas: algunas integraciones pedidas por clientes se resuelven con
  configuración o se posponen.
- Riesgo: si un cliente exige un plugin en v1.0, se negocia un módulo específico
  auditado, no un sistema abierto.

## Alternativas consideradas

| Alternativa | Motivo del rechazo |
| --- | --- |
| WASM en proceso | Aísla memoria pero comparte hilo y puede bloquear el audio. |
| DLLs nativas | Rompe el modelo de seguridad y la portabilidad del instalador. |
| Scripting embebido (Lua) | Otro runtime y otra superficie de ataque en v1.0. |
