import { LazyStore } from "@tauri-apps/plugin-store";

const STORE_FILE = "snine-ui-preferences.json";

const memory = new Map<string, string>();
let nativeStore: LazyStore | null = null;

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function initializeUiStorage(): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }

  const store = new LazyStore(STORE_FILE);
  await store.init();

  const entries = await store.entries<unknown>();

  memory.clear();

  for (const [key, value] of entries) {
    if (typeof value === "string") {
      memory.set(key, value);
    }
  }

  nativeStore = store;
}

function persistSet(key: string, value: string): void {
  const store = nativeStore;
  if (!store) return;

  void store
    .set(key, value)
    .then(() => store.save())
    .catch((error) => {
      console.warn("[SNine Launcher] Native UI preference write failed", error);
    });
}

function persistDelete(key: string): void {
  const store = nativeStore;
  if (!store) return;

  void store
    .delete(key)
    .then(() => store.save())
    .catch((error) => {
      console.warn("[SNine Launcher] Native UI preference delete failed", error);
    });
}

export const uiStorage = {
  getItem(key: string): string | null {
    return memory.get(key) ?? null;
  },

  setItem(key: string, value: string): void {
    const normalized = String(value);
    memory.set(key, normalized);
    persistSet(key, normalized);
  },

  removeItem(key: string): void {
    memory.delete(key);
    persistDelete(key);
  },
};
