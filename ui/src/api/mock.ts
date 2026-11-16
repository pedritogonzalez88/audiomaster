import type {
  AppInfo,
  Asset,
  CartSlot,
  Category,
  DspParams,
  ImportSummary,
  LibraryQuery,
  PlayoutSnapshot,
  RecoverySnapshot,
  TransportCommand,
} from "../types";

const categories: Category[] = [
  { id: 1, code: "MUSIC", name: "Música", colorHex: "#3b82f6", isCommercial: false },
  { id: 2, code: "JINGLE", name: "Jingles", colorHex: "#8b5cf6", isCommercial: false },
  { id: 3, code: "AD", name: "Publicidad", colorHex: "#f97316", isCommercial: true },
  { id: 4, code: "WHATSAPP", name: "WhatsApp", colorHex: "#10b981", isCommercial: false },
  { id: 5, code: "INBOX", name: "Bandeja de entrada", colorHex: "#64748b", isCommercial: false },
];

function asset(
  id: number,
  categoryId: number,
  title: string,
  artist: string,
  durationMs: number,
  bpm: number,
  album: string | null,
  year: number | null,
): Asset {
  return {
    id,
    categoryId,
    title,
    artist,
    album,
    releaseYear: year,
    bpm,
    kind: "AUDIO",
    durationMs,
    gainDb: 0,
    loudnessLufs: -23,
    cueInMs: 0,
    introMs: 12_000,
    hookMs: 40_000,
    mixPointMs: durationMs - 20_000,
    fadeOutMs: durationMs - 8_000,
    cueOutMs: durationMs,
    playCount: 0,
    lastPlayedAt: null,
  };
}

const library: Asset[] = [
  asset(1, 1, "Radio Ga Ga", "Queen", 240_000, 112, "The Works", 1984),
  asset(2, 1, "Every Breath You Take", "The Police", 252_000, 117, "Synchronicity", 1983),
  asset(3, 1, "Billie Jean", "Michael Jackson", 294_000, 117, "Thriller", 1982),
  asset(4, 1, "Take On Me", "a-ha", 225_000, 169, "Hunting High and Low", 1985),
  asset(5, 2, "Jingle Identidad 98.5", "Radio Master", 8_000, 0, null, null),
  asset(6, 2, "Jingle Hora Exacta", "Radio Master", 6_500, 0, null, null),
  asset(7, 3, "Supermercado Central", "Publicidad", 30_000, 0, null, null),
  asset(8, 3, "Farmacia del Pueblo", "Publicidad", 25_000, 0, null, null),
  asset(9, 4, "Saludo oyente WhatsApp", "Oyente", 14_000, 0, null, null),
  asset(10, 1, "Sweet Dreams", "Eurythmics", 216_000, 126, "Sweet Dreams", 1983),
];

const cartAssets = [5, 6, 7, 8, 9, 1, 2, 10];

let queue: Asset[] = [library[1], library[2], library[9], library[6]];
let current: Asset | null = library[0];
let positionMs = 102_000;
let playing = true;
let paused = false;
let stopAfterCurrent = false;
let talkOver = false;
let loopCurrent = false;
let voiceOver = false;
let lastTick = Date.now();
let nextId = 100;

function tick(): void {
  const now = Date.now();
  const delta = Math.max(0, now - lastTick);
  lastTick = now;
  if (!playing || paused || !current) {
    return;
  }
  positionMs += delta;
  if (positionMs >= current.durationMs) {
    const next = queue.shift() ?? null;
    current = next;
    positionMs = 0;
    if (!next) {
      playing = false;
      paused = false;
    } else if (stopAfterCurrent) {
      stopAfterCurrent = false;
      playing = false;
      paused = true;
    }
  }
}

export function mockSnapshot(): PlayoutSnapshot {
  tick();
  return {
    isPlaying: playing && !paused,
    isPaused: paused,
    stopAfterCurrent,
    currentTitle: current?.title ?? null,
    currentArtist: current?.artist ?? null,
    positionMs: current ? positionMs : 0,
    durationMs: current?.durationMs ?? 0,
    nextTitle: queue[0]?.title ?? null,
    queueLen: queue.length,
    talkOver,
    underruns: 0,
    loopCurrent,
    voiceOver,
    crossfadeProgress: null,
    incomingTitle: null,
    outgoingGain: 1,
    incomingGain: 0,
  };
}

export function mockAppInfo(): AppInfo {
  return {
    stationName: "Radio Master FM",
    version: "0.1.0 · mock",
    deviceName: "Navegador (sin audio)",
    sampleRate: 48_000,
    channels: 2,
    databasePath: ":memory:",
  };
}

export function mockCategories(): Category[] {
  return categories;
}

export function mockSearch(query: LibraryQuery): Asset[] {
  const text = query.text.trim().toLowerCase();
  return library
    .filter((item) => {
      const matchesText =
        !text ||
        item.title.toLowerCase().includes(text) ||
        item.artist.toLowerCase().includes(text) ||
        String(item.bpm).includes(text);
      const matchesCategory = query.categoryId === null || item.categoryId === query.categoryId;
      return matchesText && matchesCategory;
    })
    .slice(query.offset, query.offset + query.limit);
}

export function mockQueue(): Asset[] {
  return [...queue];
}

export function mockCartSlots(page: number): CartSlot[] {
  return cartAssets.map((assetId, index) => {
    const item = library.find((candidate) => candidate.id === assetId) ?? null;
    return {
      id: page * 100 + index + 1,
      slotIndex: index + 1,
      hotkey: index < 9 ? `F${index + 1}` : `Shift+F${index - 8}`,
      colorHex: item ? categories.find((cat) => cat.id === item.categoryId)!.colorHex : "#64748b",
      isLoop: false,
      gainDb: 0,
      assetId: item?.id ?? null,
      title: item?.title ?? null,
      artist: item?.artist ?? null,
      durationMs: item?.durationMs ?? null,
    };
  });
}

export function mockCommand(command: TransportCommand): PlayoutSnapshot {
  tick();
  switch (command) {
    case "PLAY":
      if (!current && queue.length > 0) {
        current = queue.shift() ?? null;
        positionMs = 0;
      }
      if (current) {
        playing = true;
        paused = false;
      }
      break;
    case "PAUSE":
      if (playing) {
        paused = !paused;
      }
      break;
    case "STOP":
      current = null;
      queue = [];
      positionMs = 0;
      playing = false;
      paused = false;
      stopAfterCurrent = false;
      break;
    case "NEXT": {
      const next = queue.shift() ?? null;
      current = next;
      positionMs = 0;
      playing = next !== null;
      paused = false;
      break;
    }
    case "STOP_AFTER_CURRENT":
      stopAfterCurrent = !stopAfterCurrent;
      break;
  }
  return mockSnapshot();
}

export function mockEnqueue(assetId: number): PlayoutSnapshot {
  const item = library.find((candidate) => candidate.id === assetId);
  if (item) {
    queue.push(item);
    if (!current) {
      current = queue.shift() ?? null;
      positionMs = 0;
    }
  }
  return mockSnapshot();
}

export function mockTriggerCart(assetId: number, loop = false, gainDb?: number): PlayoutSnapshot {
  const item = library.find((candidate) => candidate.id === assetId);
  if (item) {
    if (gainDb !== undefined) {
      item.gainDb = gainDb;
    }
    queue.unshift(item);
    if (!current) {
      current = queue.shift() ?? null;
      positionMs = 0;
    }
    playing = current !== null;
    paused = false;
    loopCurrent = loop;
  }
  return mockSnapshot();
}

export function mockInstantPlay(assetId: number): PlayoutSnapshot {
  const item = library.find((candidate) => candidate.id === assetId);
  if (item) {
    queue.unshift(item);
    if (!current) {
      current = queue.shift() ?? null;
      positionMs = 0;
    } else {
      // simulate immediate crossfade: treat as incoming
    }
    playing = current !== null;
    paused = false;
  }
  return mockSnapshot();
}

export function mockPlayOver(assetId: number): PlayoutSnapshot {
  const item = library.find((candidate) => candidate.id === assetId);
  if (item) {
    queue.unshift(item);
    if (!current) {
      current = queue.shift() ?? null;
      positionMs = 0;
    }
    playing = current !== null;
    paused = false;
    voiceOver = true;
    talkOver = true;
  }
  return mockSnapshot();
}

export function mockSetLoop(enabled: boolean): PlayoutSnapshot {
  loopCurrent = enabled;
  return mockSnapshot();
}

export function mockStopCart(assetId: number): PlayoutSnapshot {
  if (current?.id === assetId) {
    current = null;
    positionMs = 0;
    playing = false;
    paused = false;
    loopCurrent = false;
  }
  return mockSnapshot();
}

export function mockClearQueue(): PlayoutSnapshot {
  queue = [];
  current = null;
  positionMs = 0;
  playing = false;
  paused = false;
  stopAfterCurrent = false;
  talkOver = false;
  loopCurrent = false;
  voiceOver = false;
  return mockSnapshot();
}

export function mockImportFolder(): ImportSummary {
  return { imported: 3, skipped: 1, failed: 0, total: 4 };
}

export function mockImport(path: string, categoryId: number): Asset {
  const name =
    path
      .split(/[\\/]/)
      .pop()
      ?.replace(/\.[^.]+$/, "") ?? "Importado";
  const created = asset(nextId++, categoryId, name, "Artista desconocido", 180_000, 0, null, null);
  library.unshift(created);
  return created;
}

export function mockDspParams(): DspParams {
  return {
    eqGainsDb: Array(10).fill(0),
    compThresholdDb: -18,
    compRatio: 4,
    compAttackMs: 10,
    compReleaseMs: 100,
    limiterCeilingDb: -1,
    enabled: true,
  };
}

export function mockToggleTalkOver(): PlayoutSnapshot {
  talkOver = !talkOver;
  return mockSnapshot();
}

export function mockRecoveryStatus(): RecoverySnapshot | null {
  return null;
}

export function mockRestoreRecovery(): PlayoutSnapshot {
  return mockSnapshot();
}
