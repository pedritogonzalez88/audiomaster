# ADR-001 — Framework UI: React 19 + TypeScript

- **Estado:** Aceptado
- **Fecha:** 2026-11-15
- **Decisores:** Arquitectura, Frontend

## Contexto

La UI de operación en vivo necesita: actualización de telemetría a 20-30 Hz,
búsqueda instantánea sobre 50.000 filas, tablas/listas largas (biblioteca, cola,
auditoría), atajos de teclado globales, múltiples temas (oscuro, claro, alto
contraste) y una consola con medidores que se repintan constantemente. El
prototipo de alta fidelidad ya está escrito en HTML + Tailwind, de modo que la
ruta más corta es un framework de componentes con tipado estático.

## Decisión

- **React 19** con **TypeScript 5.7** en modo `strict` (incluye `noUnusedLocals`
  y `noUnusedParameters`).
- **Vite 6** como bundler y dev server (`strictPort: 5173`, requerido por Tauri).
- **Vitest 3** para tests de lógica pura; los componentes se testean a nivel de
  comportamiento cuando la lógica lo justifica.
- **Sin Tailwind CDN**: los tokens del prototipo se extraen a CSS variables
  (`ui/src/design-system/tokens.css`) y los estilos viven en `styles.css`. Esto
  elimina una dependencia de red en runtime y permite temas por `data-theme`.
- El estado del servidor se lee por *polling* (2 Hz) desde Tauri; no se agrega
  un cliente de caché externo en v1.0.

## Consecuencias

- Positivas: ecosistema maduro, tipado compartido con los DTO de Rust, build
  reproducido por Vite, hot reload rápido durante el desarrollo.
- Negativas: el polling a 2 Hz limita la latencia visual a ~500 ms; los medidores
  interpolan localmente. Si la telemetría necesita más resolución se evalúa un
  canal de eventos (ADR-011).
- Riesgo aceptado: React 19 es reciente; se fija la versión en `package.json` y
  el lockfile.

## Alternativas consideradas

| Alternativa | Motivo del rechazo |
| --- | --- |
| Svelte 5 | Ecosistema de componentes de audio/medidores más chico. |
| SolidJS | Menor disponibilidad de perfiles y ejemplos para el equipo. |
| Vanilla + Web Components | Sin tipado de plantillas; el estado complejo se vuelve manual. |
| Qt / egui (UI nativa) | Perdería el prototipo existente y el theming CSS. |

## Verificación

`npm run typecheck`, `npm run lint`, `npm run test` y `npm run build` pasan en CI
para Windows, macOS y Ubuntu.
