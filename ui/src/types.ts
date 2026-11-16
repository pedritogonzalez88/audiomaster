export type AssetKind = "AUDIO" | "VIDEO";

export interface AppInfo {
  stationName: string;
  version: string;
  deviceName: string;
  sampleRate: number;
  channels: number;
  databasePath: string;
}

export interface Category {
  id: number;
  code: string;
  name: string;
  colorHex: string;
  isCommercial: boolean;
}

export interface Asset {
  id: number;
  categoryId: number;
  title: string;
  artist: string;
  album: string | null;
  releaseYear: number | null;
  bpm: number;
  kind: AssetKind;
  durationMs: number;
  gainDb: number;
  loudnessLufs: number;
  cueInMs: number;
  introMs: number;
  hookMs: number;
  mixPointMs: number;
  fadeOutMs: number;
  cueOutMs: number;
  playCount: number;
  lastPlayedAt: number | null;
}

export interface PlayoutSnapshot {
  isPlaying: boolean;
  isPaused: boolean;
  stopAfterCurrent: boolean;
  currentTitle: string | null;
  currentArtist: string | null;
  positionMs: number;
  durationMs: number;
  nextTitle: string | null;
  queueLen: number;
  talkOver: boolean;
  underruns: number;
  loopCurrent: boolean;
  voiceOver: boolean;
  crossfadeProgress: number | null;
  incomingTitle: string | null;
  outgoingGain: number;
  incomingGain: number;
}

export interface CartSlot {
  id: number;
  slotIndex: number;
  hotkey: string;
  colorHex: string;
  isLoop: boolean;
  gainDb: number;
  assetId: number | null;
  title: string | null;
  artist: string | null;
  durationMs: number | null;
}

export interface LibraryQuery {
  text: string;
  categoryId: number | null;
  limit: number;
  offset: number;
}

export type TransportCommand = "PLAY" | "PAUSE" | "STOP" | "NEXT" | "STOP_AFTER_CURRENT";

export interface IpcError {
  code: string;
  message: string;
}

export interface DspParams {
  eqGainsDb: number[];
  compThresholdDb: number;
  compRatio: number;
  compAttackMs: number;
  compReleaseMs: number;
  limiterCeilingDb: number;
  enabled: boolean;
}

export interface ImportSummary {
  imported: number;
  skipped: number;
  failed: number;
  total: number;
}

export interface Telemetry {
  positionMs: number;
  peakL: number;
  peakR: number;
  lufs: number;
  underruns: number;
}

export interface RecoverySnapshot {
  atUtcMs: number;
  currentAssetId: number | null;
  currentPositionMs: number;
  queue: number[];
  stopAfterCurrent: boolean;
  talkOver: boolean;
}

export interface AuditEvent {
  id: number;
  at: number;
  kind: "INFO" | "PLAY" | "QUEUE" | "WARNING" | "AUDIT";
  message: string;
}

export const EMPTY_SNAPSHOT: PlayoutSnapshot = {
  isPlaying: false,
  isPaused: false,
  stopAfterCurrent: false,
  currentTitle: null,
  currentArtist: null,
  positionMs: 0,
  durationMs: 0,
  nextTitle: null,
  queueLen: 0,
  talkOver: false,
  underruns: 0,
  loopCurrent: false,
  voiceOver: false,
  crossfadeProgress: null,
  incomingTitle: null,
  outgoingGain: 1,
  incomingGain: 0,
};
