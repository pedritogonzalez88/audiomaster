/** Formats milliseconds as `mm:ss`, clamped to zero. */
export function formatDuration(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

/** Formats a countdown as `-mm:ss` (or `+mm:ss` when overtime). */
export function formatCountdown(remainingMs: number): string {
  const sign = remainingMs < 0 ? "+" : "-";
  return `${sign}${formatDuration(Math.abs(remainingMs))}`;
}

export function formatClock(date: Date): string {
  return date.toLocaleTimeString("es-PY", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

export function formatDate(date: Date): string {
  return date
    .toLocaleDateString("es-PY", {
      weekday: "long",
      day: "2-digit",
      month: "short",
      year: "numeric",
    })
    .toUpperCase();
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function progressPercent(positionMs: number, durationMs: number): number {
  if (durationMs <= 0) {
    return 0;
  }
  return clamp((positionMs / durationMs) * 100, 0, 100);
}

/** Deterministic 32-bit hash so the placeholder waveform is stable per track. */
export function hashString(value: string): number {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

/**
 * Builds a stable pseudo-waveform for the deck placeholder.
 * Values are normalized to 0..1; once real peaks are persisted this is replaced
 * by the `waveform_peaks` blob.
 */
export function buildWaveform(seed: string, bars = 96): number[] {
  let state = hashString(seed) || 1;
  const values: number[] = [];
  for (let index = 0; index < bars; index += 1) {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    state >>>= 0;
    const noise = (state % 1000) / 1000;
    // Smooth envelope: louder in the middle, quiet at the edges.
    const envelope = Math.sin((index / bars) * Math.PI) ** 0.6;
    values.push(clamp(0.12 + noise * 0.88 * envelope, 0.05, 1));
  }
  return values;
}

export function initials(value: string): string {
  return value
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? "")
    .join("");
}
