import { create } from "zustand";
import { loadShellSettings, saveShellSettings, typedIpcError } from "../lib/shellCommands";
import { applyShellTheme } from "../theme/applyTheme";
import { DEFAULT_SHELL_SETTINGS, type ShellSettings } from "../theme/types";
import type { TranslationKey } from "../i18n/messages";

export type ShellPage = "home" | "library" | "discover" | "cosmetics" | "tasks" | "updates" | "accounts" | "settings" | "diagnostics";
export type ToastTone = "success" | "warning" | "error" | "info";

export interface ToastMessage {
  id: number;
  tone: ToastTone;
  messageKey: TranslationKey;
  params?: Record<string, string | number>;
}

interface ShellState {
  page: ShellPage;
  settings: ShellSettings;
  initialized: boolean;
  loading: boolean;
  taskCenterOpen: boolean;
  mobileNavigationOpen: boolean;
  dialog: "reset-settings" | null;
  toasts: ToastMessage[];
  bootstrap: () => Promise<void>;
  setPage: (page: ShellPage) => void;
  setTaskCenterOpen: (open: boolean) => void;
  setMobileNavigationOpen: (open: boolean) => void;
  setDialog: (dialog: ShellState["dialog"]) => void;
  saveSettings: (settings: ShellSettings, successKey?: TranslationKey | null) => Promise<boolean>;
  resetSettings: () => Promise<boolean>;
  dismissToast: (id: number) => void;
}

let toastId = 0;

function markShellReady(): void {
  if (performance.getEntriesByName("s9lab.shell.ready").length === 0) {
    performance.mark("s9lab.shell.ready");
    const start = performance.getEntriesByName("s9lab.app.start")[0];
    if (start) performance.measure("s9lab.app.start-to-shell-ready", "s9lab.app.start", "s9lab.shell.ready");
  }
  requestAnimationFrame(() => performance.mark("s9lab.shell.interactive"));
}

export const useShellStore = create<ShellState>((set, get) => ({
  page: "home",
  settings: { ...DEFAULT_SHELL_SETTINGS },
  initialized: false,
  loading: false,
  taskCenterOpen: false,
  mobileNavigationOpen: false,
  dialog: null,
  toasts: [],

  bootstrap: async () => {
    set({ loading: true });
    try {
      const settings = await loadShellSettings();
      applyShellTheme(settings);
      set({ settings, initialized: true, loading: false });
      markShellReady();
    } catch (error) {
      const typed = typedIpcError(error);
      set((state) => ({
        initialized: true,
        loading: false,
        toasts: [...state.toasts, {
          id: ++toastId,
          tone: "error",
          messageKey: (typed?.messageKey as TranslationKey | undefined) ?? "error.internal_error",
          params: typed?.params,
        }],
      }));
      markShellReady();
    }
  },

  setPage: (page) => {
    performance.mark("s9lab.navigation.start");
    set({ page, mobileNavigationOpen: false });
    requestAnimationFrame(() => {
      performance.mark("s9lab.navigation.end");
      performance.measure("s9lab.navigation", "s9lab.navigation.start", "s9lab.navigation.end");
    });
  },
  setTaskCenterOpen: (taskCenterOpen) => set({ taskCenterOpen }),
  setMobileNavigationOpen: (mobileNavigationOpen) => set({ mobileNavigationOpen }),
  setDialog: (dialog) => set({ dialog }),

  saveSettings: async (settings, successKey = null) => {
    applyShellTheme(settings);
    set({ settings, loading: true });
    try {
      const saved = await saveShellSettings(settings);
      applyShellTheme(saved);
      set((state) => ({
        settings: saved,
        loading: false,
        toasts: successKey ? [...state.toasts, { id: ++toastId, tone: "success", messageKey: successKey }] : state.toasts,
      }));
      return true;
    } catch (error) {
      const previous = get().settings;
      applyShellTheme(previous);
      const typed = typedIpcError(error);
      set((state) => ({
        loading: false,
        toasts: [...state.toasts, {
          id: ++toastId,
          tone: "error",
          messageKey: (typed?.messageKey as TranslationKey | undefined) ?? "settings.saveFailed",
          params: typed?.params,
        }],
      }));
      return false;
    }
  },

  resetSettings: async () => {
    const locale = get().settings.locale;
    const success = await get().saveSettings({ ...DEFAULT_SHELL_SETTINGS, locale }, "settings.resetDone");
    if (success) set({ dialog: null });
    return success;
  },

  dismissToast: (id) => set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) })),
}));
