import { formatDuration } from "../lib/format";
import type { Asset } from "../types";

interface QueuePanelProps {
  queue: Asset[];
  onClear: () => void;
  onEnqueueAll: () => void;
  busy: boolean;
}

export function QueuePanel({ queue, onClear, onEnqueueAll, busy }: QueuePanelProps) {
  const totalMs = queue.reduce((sum, asset) => sum + asset.durationMs, 0);

  return (
    <section className="panel queue" aria-label="Playlist en aire">
      <div className="panel__header">
        <div className="panel__title">
          <span aria-hidden="true">📋</span> Playlist en Aire
        </div>
        <span className="panel__count">
          {queue.length} ítems · {formatDuration(totalMs)} restantes
        </span>
      </div>

      <div className="queue__toolbar">
        <button
          type="button"
          className="btn btn--small btn--warning"
          onClick={onEnqueueAll}
          disabled={busy}
        >
          + Insertar tanda
        </button>
        <button
          type="button"
          className="btn btn--small btn--ghost"
          onClick={onClear}
          disabled={busy}
        >
          Limpiar
        </button>
      </div>

      <ol className="queue__list">
        {queue.length === 0 ? (
          <li className="empty-state">
            La cola está vacía. Agregá pistas desde la biblioteca o el cart wall.
          </li>
        ) : (
          queue.map((asset, index) => (
            <li key={`${asset.id}-${index}`} className="queue-row">
              <span className="queue-row__position">{String(index + 1).padStart(2, "0")}</span>
              <span className="queue-row__kind" aria-hidden="true">
                {asset.kind === "VIDEO" ? "🎬" : "🎵"}
              </span>
              <span className="queue-row__body">
                <span className="queue-row__title">{asset.title}</span>
                <span className="queue-row__artist">{asset.artist || "Sin artista"}</span>
              </span>
              <span className="queue-row__duration">{formatDuration(asset.durationMs)}</span>
            </li>
          ))
        )}
      </ol>

      <div className="panel__footer">
        <span className="panel__status">
          <span className="led led--ok" aria-hidden="true" /> Ajuste horario automático
        </span>
        <span>Crossfade predeterminado: 3.5 s</span>
      </div>
    </section>
  );
}
