import { useEffect, useRef } from "react";
import { formatClock } from "../lib/format";
import type { AuditEvent } from "../types";

interface AuditFeedProps {
  events: AuditEvent[];
  onExport: () => void;
}

export function AuditFeed({ events, onExport }: AuditFeedProps) {
  const listRef = useRef<HTMLOListElement>(null);

  useEffect(() => {
    const list = listRef.current;
    if (list) {
      list.scrollTop = list.scrollHeight;
    }
  }, [events.length]);

  return (
    <section className="panel audit" aria-label="Auditoría y scheduler">
      <div className="panel__header">
        <div className="panel__title">
          <span aria-hidden="true">📡</span> Scheduler y Auditoría
        </div>
        <span className="panel__count panel__count--ok">DB LOG OK</span>
      </div>

      <ol className="audit__list" ref={listRef} aria-live="polite">
        {events.length === 0 ? (
          <li className="empty-state">Sin eventos todavía.</li>
        ) : (
          events.map((event) => (
            <li key={event.id} className={`audit-row audit-row--${event.kind.toLowerCase()}`}>
              <time>{formatClock(new Date(event.at))}</time>
              <span className="audit-row__kind">{event.kind}</span>
              <span className="audit-row__message">{event.message}</span>
            </li>
          ))
        )}
      </ol>

      <div className="panel__footer">
        <span>
          Próx. evento: <strong>15:00 Tanda #3</strong>
        </span>
        <button type="button" className="link-btn" onClick={onExport}>
          Exportar XLS/PDF
        </button>
      </div>
    </section>
  );
}
