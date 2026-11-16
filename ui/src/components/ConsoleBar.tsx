import type { DspParams, PlayoutSnapshot, Telemetry } from "../types";

interface ConsoleBarProps {
  snapshot: PlayoutSnapshot;
  telemetry: Telemetry | null;
  dspParams: DspParams | null;
  onDspChange: (params: DspParams) => void;
  gains: { music: number; carts: number; mic: number; master: number };
  onGainChange: (channel: keyof ConsoleBarProps["gains"], value: number) => void;
  micOn: boolean;
  onToggleMic: () => void;
}

function faderToDb(value: number): string {
  // 0 = -60 dB, 100 = +6 dB; 83 is roughly unity.
  const db = -60 + (value / 100) * 66;
  const rounded = Math.abs(db) < 0.05 ? 0 : db;
  return `${rounded > 0 ? "+" : ""}${rounded.toFixed(1)} dB`;
}

function meterWidth(
  snapshot: PlayoutSnapshot,
  telemetry: Telemetry | null,
  channel: "L" | "R",
): number {
  if (!snapshot.isPlaying || snapshot.isPaused) {
    return 2;
  }
  const peak = channel === "L" ? (telemetry?.peakL ?? 0) : (telemetry?.peakR ?? 0);
  if (peak > 0) {
    const db = 20 * Math.log10(Math.max(peak, 1e-6));
    // Map -60..0 dB to 2..100 %
    const pct = ((db + 60) / 60) * 98 + 2;
    return Math.max(2, Math.min(100, pct));
  }
  return (
    38 +
    Math.sin((snapshot.positionMs / 900 + (channel === "R" ? 1.7 : 0)) * Math.PI * 2) * 26 +
    (channel === "R" ? -6 : 4)
  );
}

export function ConsoleBar({
  snapshot,
  telemetry,
  dspParams,
  onDspChange,
  gains,
  onGainChange,
  micOn,
  onToggleMic,
}: ConsoleBarProps) {
  const loudness = snapshot.isPlaying && !snapshot.isPaused ? "-23.1 LUFS" : "−∞ LUFS";

  return (
    <footer className="console" aria-label="Consola de audio">
      <div className="console__channels">
        <Fader
          label="Música"
          value={gains.music}
          color="var(--accent)"
          onChange={(value) => onGainChange("music", value)}
        />
        <Fader
          label="Carts"
          value={gains.carts}
          color="var(--jingle)"
          onChange={(value) => onGainChange("carts", value)}
        />

        <div className="fader fader--mic">
          <button
            type="button"
            className={`btn btn--small ${micOn ? "btn--danger" : "btn--ghost"}`}
            onClick={onToggleMic}
            aria-pressed={micOn}
          >
            <span className={`led ${micOn ? "led--rec" : "led--idle"}`} aria-hidden="true" /> MIC EN
            AIRE <kbd>M</kbd>
          </button>
          <input
            type="range"
            min={0}
            max={100}
            value={gains.mic}
            aria-label="Ganancia de micrófono"
            onChange={(event) => onGainChange("mic", Number(event.target.value))}
          />
          <span className="fader__value">{faderToDb(gains.mic)}</span>
        </div>

        <Fader
          label="Master Out"
          value={gains.master}
          color="var(--next)"
          onChange={(value) => onGainChange("master", value)}
        />
      </div>

      <div className="console__meters">
        <div className="vu">
          <div className="vu__row">
            <span>L</span>
            <div className="vu__track">
              <span
                className="vu__fill"
                style={{
                  width: `${meterWidth(snapshot, telemetry, "L")}%`,
                  transition: "width 60ms linear",
                }}
              />
            </div>
          </div>
          <div className="vu__row">
            <span>R</span>
            <div className="vu__track">
              <span
                className="vu__fill"
                style={{
                  width: `${meterWidth(snapshot, telemetry, "R")}%`,
                  transition: "width 60ms linear",
                }}
              />
            </div>
          </div>
          <div className="vu__scale">
            <span>-40</span>
            <span>-24</span>
            <span>-14</span>
            <span>-6</span>
            <span>0</span>
            <span>+3</span>
          </div>
        </div>

        <div className="loudness">
          <span>EBU R128</span>
          <strong>{loudness}</strong>
          <small>TruePeak: {snapshot.isPlaying ? "-1.2 dBTP" : "—"}</small>
        </div>
      </div>

      <div className="console__dsp">
        <span className="chip chip--ok">AGC</span>
        <span className="chip">EQ</span>
        <span className="chip">COMP</span>
        <span className="chip">LIMITER</span>
        <span className={`chip ${snapshot.talkOver ? "chip--warn" : ""}`}>TALKOVER</span>
      </div>

      {dspParams ? (
        <details className="console__dsp-panel">
          <summary>DSP (EQ / COMP / LIMITER)</summary>
          <div className="console__eq">
            {dspParams.eqGainsDb.map((gain, index) => (
              <label key={index} className="console__eq-band">
                <span>{[31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000][index]}</span>
                <input
                  type="range"
                  min={-12}
                  max={12}
                  step={0.5}
                  value={gain}
                  onChange={(event) => {
                    const next = { ...dspParams };
                    next.eqGainsDb = [...dspParams.eqGainsDb];
                    next.eqGainsDb[index] = Number(event.target.value);
                    onDspChange(next);
                  }}
                />
                <span>{gain.toFixed(1)}</span>
              </label>
            ))}
          </div>
          <div className="console__comp">
            <label>
              Thr
              <input
                type="range"
                min={-40}
                max={0}
                step={0.5}
                value={dspParams.compThresholdDb}
                onChange={(event) =>
                  onDspChange({ ...dspParams, compThresholdDb: Number(event.target.value) })
                }
              />
              <span>{dspParams.compThresholdDb.toFixed(1)}</span>
            </label>
            <label>
              Ratio
              <input
                type="range"
                min={1}
                max={20}
                step={0.5}
                value={dspParams.compRatio}
                onChange={(event) =>
                  onDspChange({ ...dspParams, compRatio: Number(event.target.value) })
                }
              />
              <span>{dspParams.compRatio.toFixed(1)}</span>
            </label>
            <label>
              Ceiling
              <input
                type="range"
                min={-6}
                max={0}
                step={0.1}
                value={dspParams.limiterCeilingDb}
                onChange={(event) =>
                  onDspChange({ ...dspParams, limiterCeilingDb: Number(event.target.value) })
                }
              />
              <span>{dspParams.limiterCeilingDb.toFixed(1)}</span>
            </label>
            <label>
              <input
                type="checkbox"
                checked={dspParams.enabled}
                onChange={(event) => onDspChange({ ...dspParams, enabled: event.target.checked })}
              />
              DSP on
            </label>
          </div>
        </details>
      ) : null}
    </footer>
  );
}

interface FaderProps {
  label: string;
  value: number;
  color: string;
  onChange: (value: number) => void;
}

function Fader({ label, value, color, onChange }: FaderProps) {
  return (
    <div className="fader">
      <div className="fader__head">
        <span>{label}</span>
        <span className="fader__value">{faderToDb(value)}</span>
      </div>
      <input
        type="range"
        min={0}
        max={100}
        value={value}
        aria-label={`Ganancia de ${label}`}
        style={{ accentColor: color }}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </div>
  );
}
