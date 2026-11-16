import { useCallback, useEffect, useRef, useState } from "react";
import { AuditFeed } from "./components/AuditFeed";
import { CartWall } from "./components/CartWall";
import { ConsoleBar } from "./components/ConsoleBar";
import { DeckPanel } from "./components/DeckPanel";
import { LibraryPanel } from "./components/LibraryPanel";
import { QueuePanel } from "./components/QueuePanel";
import { ShortcutsDialog } from "./components/ShortcutsDialog";
import { TopBar, type Theme } from "./components/TopBar";
import { api, pickMediaFile, pickMediaFolder } from "./api";
import { listen } from "@tauri-apps/api/event";
import {
  EMPTY_SNAPSHOT,
  type AppInfo,
  type Asset,
  type AuditEvent,
  type CartSlot,
  type Category,
  type DspParams,
  type PlayoutSnapshot,
  type RecoverySnapshot,
  type Telemetry,
  type TransportCommand,
} from "./types";

const THEMES: Theme[] = ["dark", "light", "contrast"];

function messageOf(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

export default function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [categories, setCategories] = useState<Category[]>([]);
  const [assets, setAssets] = useState<Asset[]>([]);
  const [queue, setQueue] = useState<Asset[]>([]);
  const [snapshot, setSnapshot] = useState<PlayoutSnapshot>(EMPTY_SNAPSHOT);
  const [telemetry, setTelemetry] = useState<Telemetry | null>(null);
  const [dspParams, setDspParams] = useState<DspParams | null>(null);
  const [recovery, setRecovery] = useState<RecoverySnapshot | null>(null);
  const [cartSlots, setCartSlots] = useState<CartSlot[]>([]);
  const [cartPage, setCartPage] = useState(1);

  const [query, setQuery] = useState("");
  const [selectedCategory, setSelectedCategory] = useState<number | null>(null);
  const [searchMs, setSearchMs] = useState<number | null>(null);
  const [libraryVersion, setLibraryVersion] = useState(0);

  const [mode, setMode] = useState<"AUTO" | "ASSIST">("AUTO");
  const [theme, setTheme] = useState<Theme>(() => {
    const stored =
      typeof localStorage === "undefined" ? null : localStorage.getItem("audiomaster.theme");
    return THEMES.includes(stored as Theme) ? (stored as Theme) : "dark";
  });
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [onAir, setOnAir] = useState(true);
  const [micOn, setMicOn] = useState(false);
  const [gains, setGains] = useState({ music: 85, carts: 90, mic: 75, master: 95 });
  const [events, setEvents] = useState<AuditEvent[]>([]);
  const [busy, setBusy] = useState(false);
  const [toast, setToast] = useState<{ kind: "ok" | "warn"; message: string } | null>(null);
  const eventId = useRef(1);

  const pushEvent = useCallback((kind: AuditEvent["kind"], message: string) => {
    setEvents((current) => {
      const next: AuditEvent = {
        id: eventId.current++,
        at: Date.now(),
        kind,
        message,
      };
      return [...current.slice(-149), next];
    });
  }, []);

  const notifyError = useCallback(
    (error: unknown) => {
      const message = messageOf(error);
      setToast({ kind: "warn", message });
      pushEvent("WARNING", message);
    },
    [pushEvent],
  );

  const refreshQueue = useCallback(async () => {
    setQueue(await api.queue());
  }, []);

  const command = useCallback(
    async (transport: TransportCommand) => {
      setBusy(true);
      try {
        const next = await api.command(transport);
        setSnapshot(next);
        await refreshQueue();
        const labels: Record<TransportCommand, string> = {
          PLAY: "Play",
          PAUSE: "Pausa",
          STOP: "Stop",
          NEXT: "Siguiente",
          STOP_AFTER_CURRENT: "Pausa al final",
        };
        pushEvent("PLAY", `Transporte: ${labels[transport]}`);
      } catch (error) {
        notifyError(error);
      } finally {
        setBusy(false);
      }
    },
    [notifyError, pushEvent, refreshQueue],
  );

  const enqueue = useCallback(
    async (asset: Asset) => {
      setBusy(true);
      try {
        const next = await api.enqueue(asset.id);
        setSnapshot(next);
        await refreshQueue();
        pushEvent("QUEUE", `Encolado: ${asset.artist ? `${asset.artist} — ` : ""}${asset.title}`);
      } catch (error) {
        notifyError(error);
      } finally {
        setBusy(false);
      }
    },
    [notifyError, pushEvent, refreshQueue],
  );

  const toggleTalkOver = useCallback(async () => {
    setBusy(true);
    try {
      const next = await api.toggleTalkOver();
      setSnapshot(next);
      pushEvent("INFO", next.talkOver ? "Talkover activado (-14 dB)" : "Talkover desactivado");
    } catch (error) {
      notifyError(error);
    } finally {
      setBusy(false);
    }
  }, [notifyError, pushEvent]);

  const importMedia = useCallback(async () => {
    try {
      const path = await pickMediaFile();
      if (!path) {
        return;
      }
      setBusy(true);
      const created = await api.importMedia(path, selectedCategory ?? 1);
      setLibraryVersion((version) => version + 1);
      setToast({ kind: "ok", message: `Importado: ${created.title}` });
      pushEvent("AUDIT", `Importado a biblioteca: ${created.title}`);
    } catch (error) {
      notifyError(error);
    } finally {
      setBusy(false);
    }
  }, [notifyError, pushEvent, selectedCategory]);

  const importFolder = useCallback(async () => {
    try {
      const path = await pickMediaFolder();
      if (!path) {
        return;
      }
      setBusy(true);
      const summary = await api.importFolder(path, selectedCategory ?? 1, true);
      setLibraryVersion((version) => version + 1);
      setToast({
        kind: summary.failed > 0 ? "warn" : "ok",
        message: `Carpeta: ${summary.imported} importados, ${summary.skipped} duplicados, ${summary.failed} errores`,
      });
      pushEvent("AUDIT", `Importación de carpeta: ${summary.imported}/${summary.total} nuevos`);
    } catch (error) {
      notifyError(error);
    } finally {
      setBusy(false);
    }
  }, [notifyError, pushEvent, selectedCategory]);

  const handleDspChange = useCallback(
    (params: DspParams) => {
      setDspParams(params);
      void api.setDspParams(params).catch((error) => notifyError(error));
    },
    [notifyError],
  );

  const restoreRecovery = useCallback(async () => {
    if (!recovery) {
      return;
    }
    setBusy(true);
    try {
      const next = await api.restoreRecovery(recovery);
      setSnapshot(next);
      await refreshQueue();
      setRecovery(null);
      setToast({ kind: "ok", message: "Sesión restaurada" });
      pushEvent("AUDIT", "Recuperación de sesión aplicada");
    } catch (error) {
      notifyError(error);
    } finally {
      setBusy(false);
    }
  }, [notifyError, pushEvent, recovery, refreshQueue]);

  const insertBreak = useCallback(async () => {
    setBusy(true);
    try {
      const ads = await api.search({ text: "", categoryId: 3, limit: 3, offset: 0 });
      if (ads.length === 0) {
        setToast({ kind: "warn", message: "No hay tandas cargadas en la categoría Publicidad" });
        return;
      }
      for (const ad of ads) {
        await api.enqueue(ad.id);
      }
      await refreshQueue();
      pushEvent("AUDIT", `Tanda insertada: ${ads.length} piezas`);
    } catch (error) {
      notifyError(error);
    } finally {
      setBusy(false);
    }
  }, [notifyError, pushEvent, refreshQueue]);

  const stopCart = useCallback(
    async (slot: CartSlot) => {
      if (slot.assetId === null) {
        return;
      }
      setBusy(true);
      try {
        const next = await api.stopCart(slot.assetId);
        setSnapshot(next);
        await refreshQueue();
        pushEvent("PLAY", `Cart stop: ${slot.title ?? "sin título"}`);
      } catch (error) {
        notifyError(error);
      } finally {
        setBusy(false);
      }
    },
    [notifyError, pushEvent, refreshQueue],
  );

  const triggerCart = useCallback(
    async (slot: CartSlot) => {
      if (slot.assetId === null) {
        return;
      }
      setBusy(true);
      try {
        const next = await api.triggerCart(slot.assetId, slot.isLoop, slot.gainDb);
        setSnapshot(next);
        await refreshQueue();
        pushEvent("PLAY", `Cart wall ${slot.hotkey}: ${slot.title ?? "sin título"}`);
      } catch (error) {
        notifyError(error);
      } finally {
        setBusy(false);
      }
    },
    [notifyError, pushEvent, refreshQueue],
  );

  const stopAll = useCallback(() => {
    void command("STOP");
  }, [command]);

  const clearQueue = useCallback(async () => {
    setBusy(true);
    try {
      const next = await api.clearQueue();
      setSnapshot(next);
      setQueue([]);
      pushEvent("AUDIT", "Lista de reproducción vaciada");
    } catch (error) {
      notifyError(error);
    } finally {
      setBusy(false);
    }
  }, [notifyError, pushEvent]);

  // Bootstrap: station info and categories.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const [appInfo, appCategories] = await Promise.all([api.appInfo(), api.categories()]);
        if (cancelled) {
          return;
        }
        setInfo(appInfo);
        setCategories(appCategories);
        try {
          const params = await api.getDspParams();
          if (!cancelled) {
            setDspParams(params);
          }
        } catch {
          // best-effort
        }
        try {
          const pending = await api.recoveryStatus();
          if (!cancelled) {
            setRecovery(pending);
          }
        } catch {
          // Recovery is best-effort; ignore.
        }
        pushEvent(
          "INFO",
          `Sesión iniciada · ${appInfo.stationName} · ${appInfo.sampleRate / 1000} kHz / ${appInfo.channels} ch`,
        );
      } catch (error) {
        if (!cancelled) {
          notifyError(error);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [notifyError, pushEvent]);

  // Telemetry channel (ADR-011): ~25 Hz aggregated frame, written without
  // re-rendering the whole tree.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        const fn = await listen<Telemetry>("telemetry", (event) => {
          setTelemetry(event.payload);
        });
        unlisten = fn;
      } catch {
        // Best-effort; polling remains the fallback.
      }
    })();
    return () => {
      unlisten?.();
    };
  }, []);
  useEffect(() => {
    let cancelled = false;
    const poll = async () => {
      try {
        const [nextSnapshot, nextQueue] = await Promise.all([api.snapshot(), api.queue()]);
        if (!cancelled) {
          setSnapshot(nextSnapshot);
          setQueue(nextQueue);
        }
      } catch {
        // Polling errors are transient; the command paths surface actionable errors.
      }
    };
    void poll();
    const timer = window.setInterval(poll, 500);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, []);

  // Library search with a small debounce.
  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(async () => {
      const started = performance.now();
      try {
        const results = await api.search({
          text: query,
          categoryId: selectedCategory,
          limit: 100,
          offset: 0,
        });
        if (!cancelled) {
          setAssets(results);
          setSearchMs(performance.now() - started);
        }
      } catch (error) {
        if (!cancelled) {
          notifyError(error);
        }
      }
    }, 120);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [query, selectedCategory, libraryVersion, notifyError]);

  // Cart wall page.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const slots = await api.cartSlots(cartPage);
        if (!cancelled) {
          setCartSlots(slots);
        }
      } catch (error) {
        if (!cancelled) {
          notifyError(error);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [cartPage, notifyError]);

  // Theme persistence.
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("audiomaster.theme", theme);
  }, [theme]);

  // Toast auto-dismiss.
  useEffect(() => {
    if (!toast) {
      return;
    }
    const timer = window.setTimeout(() => setToast(null), 4000);
    return () => window.clearTimeout(timer);
  }, [toast]);

  // Global keyboard shortcuts.
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable)
      ) {
        return;
      }

      if (event.key === "Escape") {
        setShortcutsOpen(false);
        return;
      }
      if (event.key === "?") {
        event.preventDefault();
        setShortcutsOpen((open) => !open);
        return;
      }
      if (event.code === "Space") {
        event.preventDefault();
        void command(snapshot.isPlaying ? "PAUSE" : "PLAY");
        return;
      }
      if (event.key === "F9") {
        event.preventDefault();
        void command("NEXT");
        return;
      }
      if (event.key.toLowerCase() === "s") {
        void command("STOP_AFTER_CURRENT");
        return;
      }
      if (event.key.toLowerCase() === "t") {
        void toggleTalkOver();
        return;
      }
      if (event.key.toLowerCase() === "m") {
        setMicOn((value) => !value);
        return;
      }

      const functionKey = /^F([1-8])$/.exec(event.key);
      if (functionKey) {
        const index = Number(functionKey[1]) - 1;
        const slot = cartSlots[index];
        if (slot) {
          event.preventDefault();
          void triggerCart(slot);
        }
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [cartSlots, command, snapshot.isPlaying, toggleTalkOver, triggerCart]);

  const cycleTheme = useCallback(() => {
    setTheme((current) => THEMES[(THEMES.indexOf(current) + 1) % THEMES.length]);
  }, []);

  return (
    <div className="app">
      {recovery ? (
        <div className="recovery-banner" role="status">
          <span>
            Se detectó una sesión anterior ({new Date(recovery.atUtcMs).toLocaleString()}).
          </span>
          <button
            type="button"
            className="btn btn--small"
            onClick={() => void restoreRecovery()}
            disabled={busy}
          >
            Reanudar
          </button>
          <button
            type="button"
            className="btn btn--small"
            onClick={() => setRecovery(null)}
            disabled={busy}
          >
            Descartar
          </button>
        </div>
      ) : null}
      <a className="skip-link" href="#main">
        Saltar al contenido
      </a>

      <TopBar
        info={info}
        snapshot={snapshot}
        mode={mode}
        onModeChange={setMode}
        theme={theme}
        onCycleTheme={cycleTheme}
        onOpenShortcuts={() => setShortcutsOpen(true)}
        onAir={onAir}
        onToggleOnAir={() => setOnAir((value) => !value)}
      />

      <main className="main" id="main">
        <LibraryPanel
          categories={categories}
          assets={assets}
          query={query}
          selectedCategory={selectedCategory}
          searchMs={searchMs}
          onQueryChange={setQuery}
          onSelectCategory={setSelectedCategory}
          onEnqueue={(asset) => void enqueue(asset)}
          onImport={() => void importMedia()}
          onImportFolder={() => void importFolder()}
          busy={busy}
        />

        <div className="center-column">
          <DeckPanel
            snapshot={snapshot}
            onCommand={(transport) => void command(transport)}
            onToggleTalkOver={() => void toggleTalkOver()}
            busy={busy}
          />
          <QueuePanel
            queue={queue}
            onClear={clearQueue}
            onEnqueueAll={() => void insertBreak()}
            busy={busy}
          />
        </div>

        <div className="right-column">
          <CartWall
            slots={cartSlots}
            page={cartPage}
            onPage={setCartPage}
            onTrigger={(slot) => void triggerCart(slot)}
            onStop={(slot) => void stopCart(slot)}
            onStopAll={stopAll}
            busy={busy}
          />
          <AuditFeed
            events={events}
            onExport={() => setToast({ kind: "ok", message: "Exportación disponible en Fase 3" })}
          />
        </div>
      </main>

      <ConsoleBar
        snapshot={snapshot}
        telemetry={telemetry}
        dspParams={dspParams}
        onDspChange={handleDspChange}
        gains={gains}
        onGainChange={(channel, value) => setGains((current) => ({ ...current, [channel]: value }))}
        micOn={micOn}
        onToggleMic={() => setMicOn((value) => !value)}
      />

      <ShortcutsDialog open={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />

      {toast ? (
        <div className={`toast toast--${toast.kind}`} role="status" aria-live="polite">
          {toast.message}
        </div>
      ) : null}
    </div>
  );
}
