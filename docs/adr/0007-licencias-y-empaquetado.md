# ADR-007 — Licencias y empaquetado de libmpv/FFmpeg; tamaño del instalador

- **Estado:** Aceptado, con revisión legal abierta (G-17)
- **Fecha:** 2026-11-15
- **Decisores:** Arquitectura, Legal, Producto

## Contexto

RNF-05 pide un instalador de **≤25 MB**. Un build típico de libmpv + FFmpeg pesa
**40-80 MB**; sumado al WebView2 runtime (distribuible de ~1.5 MB pero que en la
práctica descarga ~120 MB la primera vez) el objetivo es inalcanzable. Además,
libmpv es **GPLv2+** por defecto y FFmpeg puede habilitar componentes GPL, lo que
contamina la distribución completa si se enlaza estáticamente.

## Decisión

1. **RNF-05 se reescribe** a: instalador **≤80 MB** incluyendo video, o
   **≤30 MB** para el build "radio" sin video.
2. El instalador se publica en **dos variantes**:
   - `audiomaster-core`: sin libmpv (radio).
   - `audiomaster-video`: incluye libmpv/FFmpeg como **componente opcional
     descargable** al primer uso o desde el instalador.
3. Se usa un **build LGPL-only** de libmpv/FFmpeg: sin `--enable-gpl`, sin
   `x264`/`x265`, sin `libfdk-aac`. Se **enlaza dinámicamente** (DLL/dylib/so)
   para permitir la sustitución por el usuario y cumplir la LGPL.
4. Se incluye **SBOM** (CycloneDX) y el texto completo de licencias de terceros
   en `THIRD_PARTY_LICENSES.md`, generado en CI.
5. **VST/ASIO quedan fuera de v1.0**: sus licencias no permiten distribución
   libre y obligarían a un acuerdo separado.
6. El binario principal declara licencia `Proprietary` en `Cargo.toml` y se
   distribuye con EULA; los crates de terceros conservan sus licencias.

## Consecuencias

- Positivas: el objetivo de tamaño vuelve a ser alcanzable y medible; el build
  radio no arrastra GPL; la LGPL se cumple por enlace dinámico.
- Negativas: dos artefactos que mantener; el usuario de video paga una descarga
  extra; el SBOM debe regenerarse por release.
- **Bloqueo abierto:** falta la revisión legal de (a) enlace dinámico de libmpv,
  (b) códigocs patentados en FFmpeg (AAC/H.264) para Paraguay, (c) EULA del
  producto. Hasta cerrarlo, el artefacto de video no se publica.

## Alternativas consideradas

| Alternativa | Motivo del rechazo |
| --- | --- |
| Solo libmpv del sistema | Windows no lo trae; instalación manual inaceptable. |
| Build GPL y liberar el producto | Cambia el modelo de negocio del cliente. |
| WebCodecs del WebView | Sin soporte de contenedores ni de todos los códecs. |
| No soportar video en v1.0 | Contradice RF-01/02/03 del alcance acordado. |

## Verificación

Spike D mide el tamaño real de `audiomaster-core` y `audiomaster-video` en las
tres plataformas y verifica con `ldd`/`otool`/`dumpbin` que libmpv sea dinámico.
Ver [`docs/spikes-fase-0.md`](../spikes-fase-0.md).
