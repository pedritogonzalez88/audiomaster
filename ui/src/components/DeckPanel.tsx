import { useMemo } from "react";
import { buildWaveform, formatCountdown, formatDuration, progressPercent } from "../lib/format";
import type { PlayoutSnapshot, TransportCommand } from "../types";

interface DeckPanelProps {
  snapshot: PlayoutSnapshot;
  onCommand: (command: TransportCommand) => void;
  onToggleTalkOver: () => void;
  busy: boolean;
}

export function DeckPanel({ snapshot, onCommand, onToggleTalkOver, busy }: DeckPanelProps) {
  const seed = snapshot.currentTitle ?? "silencio";
  const waveform = useMemo(() => buildWaveform(seed, 120), [seed]);
  const progress = progressPercent(snapshot.positionMs, snapshot.durationMs);
  const remaining = snapshot.durationMs - snapshot.positionMs;
  const introPercent = snapshot.durationMs > 0 ? (45_000 / snapshot.durationMs) * 100 : 0;
  const mixPercent = snapshot.durationMs > 0 ? (18_000 / snapshot.durationMs) * 100 : 0;

  return (
    <div className={`deck ${snapshot.isPlaying ? "deck--live" : ""}`}>
      <div className="deck__ribbon">CANAL PRINCIPAL DE EMISIÓN</div>

      <div className="deck__head">
        <div className="deck__identity">
          <div className="deck__cover" aria-hidden="true">
            {snapshot.isPlaying ? <span className="deck__disc" /> : "🎵"}
          </div>
          <div className="deck__text">
            <div className="deck__badges">
              <span className="chip chip--accent">EN AIRE</span>
              <span className="chip chip--muted">
                {snapshot.talkOver ? "TALKOVER -14 dB" : "SIN TALKOVER"}
              </span>
            </div>
            <h2 className="deck__title">{snapshot.currentTitle ?? "Sin reproducción"}</h2>
            <p className="deck__artist">
              {snapshot.currentArtist ?? "Cargá una pista desde la biblioteca"}
            </p>
          </div>
        </div>

        <div className="deck__timers">
          <div className="deck__timer-row">
            <span>INTRO</span>
            <strong>{formatDuration(Math.min(snapshot.positionMs, 45_000))}</strong>
          </div>
          <div className={`deck__remaining ${remaining < 15_000 ? "is-critical" : ""}`}>
            {formatCountdown(remaining)}
          </div>
          <div className="deck__timer-row">
            <span>TRANS {formatDuration(snapshot.positionMs)}</span>
            <span>TOTAL {formatDuration(snapshot.durationMs)}</span>
          </div>
        </div>
      </div>

      <div className="waveform" aria-hidden="true">
        {waveform.map((value, index) => (
          <span
            key={index}
            className="waveform__bar"
            style={{ height: `${Math.round(value * 100)}%` }}
          />
        ))}
        <span className="waveform__played" style={{ width: `${progress}%` }} />
        <span className="waveform__playhead" style={{ left: `${progress}%` }} />
        <span className="waveform__cue waveform__cue--intro" style={{ width: `${introPercent}%` }}>
          INTRO
        </span>
        <span className="waveform__cue waveform__cue--mix" style={{ width: `${mixPercent}%` }}>
          NEXT MIX
        </span>
      </div>

      <div className="deck__transport">
        <div className="deck__transport-group">
          <button
            type="button"
            className={`btn btn--play ${snapshot.isPlaying ? "is-playing" : ""}`}
            onClick={() => onCommand("PLAY")}
            disabled={busy || !snapshot.currentTitle}
            aria-label={snapshot.isPlaying ? "Pausar" : "Reproducir"}
          >
            <span aria-hidden="true">{snapshot.isPlaying ? "⏸" : "▶"}</span>
            <span>{snapshot.isPlaying ? "PAUSAR" : "REPRODUCIR"}</span>
            <kbd>ESP</kbd>
          </button>

          <button
            type="button"
            className="btn btn--accent"
            onClick={() => onCommand("NEXT")}
            disabled={busy || snapshot.queueLen === 0}
          >
            ⏭ MEZCLAR <kbd>F9</kbd>
          </button>

          <button
            type="button"
            className={`btn ${snapshot.stopAfterCurrent ? "btn--warning" : "btn--ghost"}`}
            onClick={() => onCommand("STOP_AFTER_CURRENT")}
            disabled={busy}
            aria-pressed={snapshot.stopAfterCurrent}
          >
            ⏹ PAUSA FIN <kbd>S</kbd>
          </button>

          <button
            type="button"
            className="btn btn--ghost"
            onClick={() => onCommand("STOP")}
            disabled={busy}
          >
            STOP
          </button>
        </div>

        <div className="deck__transport-group">
          <button
            type="button"
            className={`btn btn--talk ${snapshot.talkOver ? "is-active" : ""}`}
            onClick={onToggleTalkOver}
            disabled={busy}
            aria-pressed={snapshot.talkOver}
          >
            <span className="led led--talk" aria-hidden="true" />
            TALKOVER <kbd>T</kbd>
          </button>
          <span className="deck__next">PRÓXIMO: {snapshot.nextTitle ?? "cola vacía"}</span>
        </div>
      </div>
    </div>
  );
}
