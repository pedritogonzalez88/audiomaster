import type { CSSProperties } from "react";
import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { formatDuration, initials } from "../lib/format";
import type { Asset, Category } from "../types";

interface LibraryPanelProps {
  categories: Category[];
  assets: Asset[];
  query: string;
  selectedCategory: number | null;
  searchMs: number | null;
  onQueryChange: (value: string) => void;
  onSelectCategory: (id: number | null) => void;
  onEnqueue: (asset: Asset) => void;
  onImport: () => void;
  onImportFolder: () => void;
  busy: boolean;
}

const ROW_HEIGHT = 46;

export function LibraryPanel({
  categories,
  assets,
  query,
  selectedCategory,
  searchMs,
  onQueryChange,
  onSelectCategory,
  onEnqueue,
  onImport,
  onImportFolder,
  busy,
}: LibraryPanelProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: assets.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
  });

  return (
    <section className="panel library" aria-label="Biblioteca central">
      <div className="panel__header">
        <div className="panel__title">
          <span aria-hidden="true">📂</span> Biblioteca Central
        </div>
        <span className="panel__count">{assets.length} resultados</span>
      </div>

      <div className="library__search">
        <label className="sr-only" htmlFor="library-search">
          Buscar en la biblioteca
        </label>
        <span className="library__search-icon" aria-hidden="true">
          🔍
        </span>
        <input
          id="library-search"
          type="search"
          value={query}
          placeholder="Buscar título, artista, BPM…"
          onChange={(event) => onQueryChange(event.target.value)}
          autoComplete="off"
        />
        {query ? (
          <button
            type="button"
            className="icon-btn"
            onClick={() => onQueryChange("")}
            aria-label="Limpiar búsqueda"
          >
            ✕
          </button>
        ) : null}
      </div>

      <div className="filter-pills" role="group" aria-label="Filtrar por categoría">
        <button
          type="button"
          className={selectedCategory === null ? "is-active" : ""}
          onClick={() => onSelectCategory(null)}
          aria-pressed={selectedCategory === null}
        >
          Todos
        </button>
        {categories.map((category) => (
          <button
            key={category.id}
            type="button"
            className={selectedCategory === category.id ? "is-active" : ""}
            style={{ "--pill": category.colorHex } as CSSProperties}
            onClick={() => onSelectCategory(category.id)}
            aria-pressed={selectedCategory === category.id}
          >
            {category.name}
          </button>
        ))}
      </div>

      <div className="library__list" ref={parentRef} role="list">
        {assets.length === 0 ? (
          <p className="empty-state">
            No hay pistas para esta búsqueda. Importá archivos con el botón{" "}
            <strong>Importar</strong>.
          </p>
        ) : (
          <div
            style={{
              height: `${virtualizer.getTotalSize()}px`,
              width: "100%",
              position: "relative",
            }}
          >
            {virtualizer.getVirtualItems().map((virtualRow) => {
              const asset = assets[virtualRow.index];
              const category = categories.find((candidate) => candidate.id === asset.categoryId);
              return (
                <article
                  key={asset.id}
                  className="library-row"
                  role="listitem"
                  style={
                    {
                      position: "absolute",
                      top: 0,
                      left: 0,
                      width: "100%",
                      height: `${virtualRow.size}px`,
                      transform: `translateY(${virtualRow.start}px)`,
                      "--asset-color": category?.colorHex ?? "#64748b",
                    } as CSSProperties
                  }
                >
                  <div className="library-row__cover" aria-hidden="true">
                    {initials(asset.title) || "♪"}
                  </div>
                  <div className="library-row__body">
                    <div className="library-row__title" title={asset.title}>
                      {asset.title}
                    </div>
                    <div className="library-row__meta">
                      {asset.artist || "Sin artista"}
                      {asset.bpm > 0 ? ` · ${asset.bpm} BPM` : ""}
                    </div>
                  </div>
                  <div className="library-row__side">
                    <span className="library-row__duration">
                      {formatDuration(asset.durationMs)}
                    </span>
                    <button
                      type="button"
                      className="icon-btn icon-btn--accent"
                      onClick={() => onEnqueue(asset)}
                      aria-label={`Agregar ${asset.title} a la cola`}
                      disabled={busy}
                    >
                      ▶
                    </button>
                  </div>
                </article>
              );
            })}
          </div>
        )}
      </div>

      <div className="panel__footer">
        <span>
          FTS5: <strong>{searchMs === null ? "—" : `${searchMs.toFixed(1)} ms`}</strong>
        </span>
        <button type="button" className="btn btn--small" onClick={onImport} disabled={busy}>
          Importar…
        </button>
        <button
          type="button"
          className="btn btn--small"
          onClick={onImportFolder}
          disabled={busy}
          title="Importar todos los archivos de audio de una carpeta"
        >
          Carpeta…
        </button>
      </div>
    </section>
  );
}
