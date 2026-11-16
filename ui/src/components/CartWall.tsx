import type { CSSProperties } from "react";
import { formatDuration } from "../lib/format";
import type { CartSlot } from "../types";

interface CartWallProps {
  slots: CartSlot[];
  page: number;
  onPage: (page: number) => void;
  onTrigger: (slot: CartSlot) => void;
  onStop: (slot: CartSlot) => void;
  onStopAll: () => void;
  busy: boolean;
}

export function CartWall({
  slots,
  page,
  onPage,
  onTrigger,
  onStop,
  onStopAll,
  busy,
}: CartWallProps) {
  return (
    <section className="panel cartwall" aria-label="Cart wall">
      <div className="panel__header">
        <div className="panel__title">
          <span aria-hidden="true">🔘</span> Cart Wall
        </div>
        <div className="cartwall__pages" role="group" aria-label="Páginas del cart wall">
          {[1, 2, 3].map((number) => (
            <button
              key={number}
              type="button"
              className={page === number ? "is-active" : ""}
              onClick={() => onPage(number)}
              aria-pressed={page === number}
            >
              P{number}
            </button>
          ))}
        </div>
      </div>

      <div className="cartwall__grid">
        {slots.map((slot) => (
          <button
            key={slot.id}
            type="button"
            className="cart-pad"
            style={{ "--pad-color": slot.colorHex } as CSSProperties}
            onClick={() => onTrigger(slot)}
            disabled={busy || slot.assetId === null}
            title={slot.title ?? "Slot vacío"}
          >
            <span className="cart-pad__key">{slot.hotkey}</span>
            <span className="cart-pad__title">{slot.title ?? "Vacío"}</span>
            <span className="cart-pad__meta">
              {slot.durationMs ? formatDuration(slot.durationMs) : "—"}
              {slot.isLoop ? " · LOOP" : ""}
            </span>
            {slot.assetId !== null ? (
              <span
                className="cart-pad__stop"
                role="button"
                tabIndex={0}
                aria-label={`Detener ${slot.title ?? "cart"}`}
                onClick={(event) => {
                  event.stopPropagation();
                  onStop(slot);
                }}
              >
                ■
              </span>
            ) : null}
          </button>
        ))}
      </div>

      <div className="cartwall__footer">
        <span>Disparo instantáneo · P95 ≤ 30 ms</span>
        <button
          type="button"
          className="btn btn--danger btn--small"
          onClick={onStopAll}
          disabled={busy}
        >
          STOP TODOS
        </button>
      </div>
    </section>
  );
}
