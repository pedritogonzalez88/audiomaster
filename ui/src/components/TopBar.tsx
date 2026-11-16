import { useEffect, useState } from "react";
import { formatClock, formatDate } from "../lib/format";
import type { AppInfo, PlayoutSnapshot } from "../types";

export type Theme = "dark" | "light" | "contrast";

interface TopBarProps {
  info: AppInfo | null;
  snapshot: PlayoutSnapshot;
  mode: "AUTO" | "ASSIST";
  onModeChange: (mode: "AUTO" | "ASSIST") => void;
  theme: Theme;
  onCycleTheme: () => void;
  onOpenShortcuts: () => void;
  onAir: boolean;
  onToggleOnAir: () => void;
}

export function TopBar({
  info,
  snapshot,
  mode,
  onModeChange,
  theme,
  onCycleTheme,
  onOpenShortcuts,
  onAir,
  onToggleOnAir,
}: TopBarProps) {
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  const themeLabel = theme === "dark" ? "Oscuro" : theme === "light" ? "Claro" : "Alto contraste";

  return (
    <header className="topbar">
      <div className="topbar__group">
        <div className="brand">
          <div className="brand__badge" aria-hidden="true">
            98.5
          </div>
          <div>
            <div className="brand__name">
              {info?.stationName ?? "Audiomaster"}
              <span className="chip chip--muted">ASUNCIÓN</span>
            </div>
            <div className="brand__meta">
              TAURI CORE v{info?.version ?? "0.1.0"} · {info?.deviceName ?? "iniciando audio"}
            </div>
          </div>
        </div>

        <button
          type="button"
          className={`tally ${onAir ? "tally--on" : "tally--off"}`}
          onClick={onToggleOnAir}
          aria-pressed={onAir}
          title="Conmutar estado de aire"
        >
          <span className="tally__dot" aria-hidden="true" />
          <span>{onAir ? "ON AIR" : "OFF AIR"}</span>
        </button>

        <div className="mode-switch" role="group" aria-label="Modo de operación">
          <button
            type="button"
            className={mode === "AUTO" ? "is-active" : ""}
            onClick={() => onModeChange("AUTO")}
            aria-pressed={mode === "AUTO"}
          >
            AUTOMÁTICO
          </button>
          <button
            type="button"
            className={mode === "ASSIST" ? "is-active" : ""}
            onClick={() => onModeChange("ASSIST")}
            aria-pressed={mode === "ASSIST"}
          >
            ASISTIDO
          </button>
        </div>
      </div>

      <div className="master-clock">
        <div className="master-clock__side">
          <span>{formatDate(now)}</span>
          <span className="master-clock__sync">SYNC NTP · {snapshot.underruns} underruns</span>
        </div>
        <time className="master-clock__time" dateTime={now.toISOString()}>
          {formatClock(now)}
        </time>
      </div>

      <div className="topbar__group topbar__group--end">
        <div className="telemetry">
          <span
            className={`led ${snapshot.isPlaying ? "led--ok" : "led--idle"}`}
            aria-hidden="true"
          />
          <span>
            DECODER:{" "}
            <strong>{snapshot.isPlaying ? "ACTIVO" : snapshot.isPaused ? "PAUSA" : "STOP"}</strong>
          </span>
        </div>
        <div className="telemetry">
          <span className="led led--rec" aria-hidden="true" />
          <span>
            LOGGER: <strong>ACTIVO</strong>
          </span>
        </div>
        <button
          type="button"
          className="btn btn--ghost"
          onClick={onCycleTheme}
          title="Cambiar tema"
        >
          {themeLabel}
        </button>
        <button type="button" className="btn btn--ghost" onClick={onOpenShortcuts}>
          ⌨️ Atajos
        </button>
      </div>
    </header>
  );
}
