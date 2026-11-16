import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AppInfo,
  Asset,
  CartSlot,
  Category,
  DspParams,
  ImportSummary,
  IpcError,
  LibraryQuery,
  PlayoutSnapshot,
  RecoverySnapshot,
  TransportCommand,
} from "../types";
import * as mock from "./mock";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function normalizeError(error: unknown): IpcError {
  if (typeof error === "object" && error !== null && "code" in error && "message" in error) {
    const candidate = error as IpcError;
    return { code: String(candidate.code), message: String(candidate.message) };
  }
  if (error instanceof Error) {
    return { code: "unexpected", message: error.message };
  }
  return { code: "unexpected", message: String(error) };
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) {
    return mockCall<T>(command, args);
  }
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw normalizeError(error);
  }
}

function mockCall<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  switch (command) {
    case "app_info":
      return Promise.resolve(mock.mockAppInfo() as T);
    case "list_categories":
      return Promise.resolve(mock.mockCategories() as T);
    case "search_library":
      return Promise.resolve(mock.mockSearch(args?.query as LibraryQuery) as T);
    case "queue_snapshot":
      return Promise.resolve(mock.mockQueue() as T);
    case "playout_snapshot":
      return Promise.resolve(mock.mockSnapshot() as T);
    case "playout_command":
      return Promise.resolve(mock.mockCommand(args?.command as TransportCommand) as T);
    case "enqueue_asset":
      return Promise.resolve(mock.mockEnqueue(Number(args?.assetId)) as T);
    case "trigger_cart":
      return Promise.resolve(
        mock.mockTriggerCart(
          Number(args?.assetId),
          Boolean(args?.loop),
          args?.gainDb as number | undefined,
        ) as T,
      );
    case "instant_play":
      return Promise.resolve(mock.mockInstantPlay(Number(args?.assetId)) as T);
    case "play_over":
      return Promise.resolve(mock.mockPlayOver(Number(args?.assetId)) as T);
    case "clear_queue":
      return Promise.resolve(mock.mockClearQueue() as T);
      return Promise.resolve(mock.mockCartSlots(Number(args?.page ?? 1)) as T);
    case "import_media":
      return Promise.resolve(
        mock.mockImport(String(args?.path ?? "archivo.mp3"), Number(args?.categoryId ?? 1)) as T,
      );
    case "import_folder":
      return Promise.resolve(mock.mockImportFolder() as T);
    case "toggle_talk_over":
      return Promise.resolve(mock.mockToggleTalkOver() as T);
    case "start_mic":
    case "stop_mic":
    case "mic_level":
      return Promise.resolve(0 as T);
    case "list_input_devices":
      return Promise.resolve(["Micrófono (Realtek)"] as T);
    case "get_dsp_params":
      return Promise.resolve(mock.mockDspParams() as T);
    case "set_dsp_params":
      return Promise.resolve(undefined as T);
    case "set_loop":
      return Promise.resolve(mock.mockSetLoop(Boolean(args?.enabled)) as T);
    case "stop_cart":
      return Promise.resolve(mock.mockStopCart(Number(args?.assetId)) as T);
    case "seek_playout":
      return Promise.resolve(mock.mockSnapshot() as T);
    case "recovery_status":
      return Promise.resolve(mock.mockRecoveryStatus() as T);
    case "restore_recovery":
      return Promise.resolve(mock.mockRestoreRecovery() as T);
    default:
      return Promise.reject({ code: "unknown_command", message: command });
  }
}

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  categories: () => call<Category[]>("list_categories"),
  search: (query: LibraryQuery) => call<Asset[]>("search_library", { query }),
  queue: () => call<Asset[]>("queue_snapshot"),
  snapshot: () => call<PlayoutSnapshot>("playout_snapshot"),
  command: (command: TransportCommand) => call<PlayoutSnapshot>("playout_command", { command }),
  enqueue: (assetId: number) => call<PlayoutSnapshot>("enqueue_asset", { assetId }),
  triggerCart: (assetId: number, loop?: boolean, gainDb?: number) =>
    call<PlayoutSnapshot>("trigger_cart", { assetId, loop: loop ?? false, gainDb }),
  instantPlay: (assetId: number, durationMs?: number, loop?: boolean, gainDb?: number) =>
    call<PlayoutSnapshot>("instant_play", { assetId, durationMs, loop: loop ?? false, gainDb }),
  playOver: (assetId: number, durationMs?: number) =>
    call<PlayoutSnapshot>("play_over", { assetId, durationMs }),
  setLoop: (enabled: boolean) => call<PlayoutSnapshot>("set_loop", { enabled }),
  stopCart: (assetId: number) => call<PlayoutSnapshot>("stop_cart", { assetId }),
  clearQueue: () => call<PlayoutSnapshot>("clear_queue"),
  cartSlots: (page: number) => call<CartSlot[]>("cart_slots", { page }),
  importMedia: (path: string, categoryId: number) =>
    call<Asset>("import_media", { path, categoryId }),
  importFolder: (path: string, categoryId: number, recursive: boolean) =>
    call<ImportSummary>("import_folder", { path, categoryId, recursive }),
  toggleTalkOver: () => call<PlayoutSnapshot>("toggle_talk_over"),
  startMic: (device?: string) => call<void>("start_mic", { device }),
  stopMic: () => call<void>("stop_mic"),
  micLevel: () => call<number>("mic_level"),
  listInputDevices: () => call<string[]>("list_input_devices"),
  getDspParams: () => call<DspParams>("get_dsp_params"),
  setDspParams: (params: DspParams) => call<void>("set_dsp_params", { params }),
  seek: (positionMs: number) => call<PlayoutSnapshot>("seek_playout", { positionMs }),
  recoveryStatus: () => call<RecoverySnapshot | null>("recovery_status"),
  restoreRecovery: (snapshot: RecoverySnapshot) =>
    call<PlayoutSnapshot>("restore_recovery", { snapshot }),
};

/** Opens the native picker in Tauri; falls back to a hidden input in the browser. */
export async function pickMediaFile(): Promise<string | null> {
  if (isTauri) {
    const selection = await open({
      multiple: false,
      directory: false,
      filters: [
        {
          name: "Audio y video",
          extensions: ["mp3", "flac", "wav", "ogg", "m4a", "aac", "mp4", "mkv", "webm"],
        },
      ],
    });
    return typeof selection === "string" ? selection : null;
  }

  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "audio/*,video/*";
    input.onchange = () => resolve(input.files?.[0]?.name ?? null);
    input.click();
  });
}

export async function pickMediaFolder(): Promise<string | null> {
  if (isTauri) {
    const selection = await open({ directory: true, multiple: false });
    return typeof selection === "string" ? selection : null;
  }
  return null;
}
