# ADR-003 — Video: libmpv, superficie y audio

- **Estado:** Aceptado con spike pendiente (v1.0 es radio; el video entra en Fase 4)
- **Fecha:** 2026-11-15
- **Decisores:** Arquitectura, Producto

## Contexto

Las especificaciones piden reproducción de video (music videos, tandas
televisivas) pero no definen superficie de salida, sincronía A/V ni qué pasa con
el audio del video cuando suena una cortina o el micrófono. Reproductor propio
con FFmpeg implicaría escribir un pipeline de decodificación y sincronización,
fuera del presupuesto de v1.0.

## Decisión

- **libmpv** como reproductor de video, con `hwdec=auto-safe` y sin OSD de mpv
  (los controles son nuestros).
- **Superficie:** ventana nativa separada en Fase 4 (`--wid`/`--x11-bypass-compositor`
  por plataforma), no textura embebida en el WebView. Motivo: el WebView no
  garantiza *zero-copy* ni control de vsync, y la ventana separada se puede
  enviar a un segundo monitor/HDMI.
- **Audio del video:** siempre se decodifica por separado y se enruta al mismo
  mezclador que el playout, para que el *ducking* y el talkover apliquen igual.
  mpv queda con `--ao=null` cuando el video va en aire.
- **Reloj maestro:** el reloj del motor de playout; el video se esclaviza a él.
  Si el video no puede seguirlo, se degrada a pausa en el punto de mezcla.
- **Licencia:** libmpv es GPLv2+ por defecto. Ver ADR-007 y ADR-012.

## Consecuencias

- Positivas: no se escribe un decodificador de video; soporte de códecs y
  aceleración por hardware resuelto.
- Negativas: agrega 40-80 MB al instalador si se empaqueta, lo que obliga a
  revisar RNF-05 (ver ADR-007). En Wayland la ventana nativa requiere permisos
  del compositor y no funciona en todos los entornos.
- Riesgo: la sincronía A/V con audio externo es la parte más frágil del diseño;
  debe medirse con el spike B antes de comprometer una fecha de Fase 4.

## Alternativas consideradas

| Alternativa | Motivo del rechazo |
| --- | --- |
| FFmpeg + `wgpu` propio | Semanas de trabajo y riesgo alto de deriva A/V. |
| `<video>` del WebView | Códecs limitados, sin control de salida ni de dispositivo. |
| VLC (libvlc) | Instalador mayor y licencia LGPL con módulos GPL. |

## Pendientes del spike B

1. Medir arranque en frío y tamaño de bundle en Windows/macOS/Ubuntu.
2. Verificar `hwdec` en GPU Intel/NVIDIA/AMD y en sesión Wayland vs X11.
3. Definir el protocolo de la ventana separada (posición, fullscreen, cierre).
4. Medir deriva A/V durante 30 min de video con audio por el mezclador.
