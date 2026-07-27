import fs from "node:fs";
import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
export const PROJECT_ROOT = path.resolve(scriptDirectory, "..");
export const VITE_CLI = path.join(PROJECT_ROOT, "node_modules", "vite", "bin", "vite.js");
export const PREVIEW_OUTPUT_LIMIT = 16_384;

export class PreviewAbortError extends Error {
  constructor(message, signalName) {
    super(message);
    this.name = "AbortError";
    this.code = "ERR_PREVIEW_ABORTED";
    this.signal = signalName;
  }
}

function abortError(signal) {
  if (signal?.reason instanceof Error) return signal.reason;
  return new PreviewAbortError("Browserprüfung wurde abgebrochen.");
}

function throwIfAborted(signal) {
  if (signal?.aborted) throw abortError(signal);
}

export function createProcessSignalController(emitter = process) {
  const controller = new AbortController();
  const listeners = new Map();
  for (const signalName of ["SIGINT", "SIGTERM"]) {
    const listener = () => {
      if (!controller.signal.aborted) {
        controller.abort(new PreviewAbortError(`Browserprüfung durch ${signalName} abgebrochen.`, signalName));
      }
    };
    listeners.set(signalName, listener);
    emitter.on(signalName, listener);
  }
  return {
    signal: controller.signal,
    dispose() {
      for (const [signalName, listener] of listeners) emitter.off(signalName, listener);
      listeners.clear();
    },
  };
}

function raceWithAbort(promise, signal) {
  if (!signal) return promise;
  throwIfAborted(signal);
  return new Promise((resolve, reject) => {
    const onAbort = () => reject(abortError(signal));
    signal.addEventListener("abort", onAbort, { once: true });
    Promise.resolve(promise).then(resolve, reject).finally(() => {
      signal.removeEventListener("abort", onAbort);
    });
  });
}

export function findBrowserExecutable() {
  const candidates = [
    process.env.S9LAB_BROWSER_PATH,
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
    "/usr/bin/google-chrome",
    process.env.PROGRAMFILES && path.join(process.env.PROGRAMFILES, "Microsoft", "Edge", "Application", "msedge.exe"),
    process.env["PROGRAMFILES(X86)"] && path.join(process.env["PROGRAMFILES(X86)"], "Microsoft", "Edge", "Application", "msedge.exe"),
    process.env.LOCALAPPDATA && path.join(process.env.LOCALAPPDATA, "Google", "Chrome", "Application", "chrome.exe"),
  ].filter(Boolean);
  const executable = candidates.find((candidate) => fs.existsSync(candidate));
  if (!executable) throw new Error("No supported local Chromium, Chrome or Edge executable found. Set S9LAB_BROWSER_PATH.");
  return executable;
}

export function appendBoundedOutput(current, chunk, limit = PREVIEW_OUTPUT_LIMIT) {
  const combined = `${current}${chunk.toString()}`;
  return combined.length <= limit ? combined : combined.slice(-limit);
}

export async function findAvailablePort(host = "127.0.0.1") {
  return new Promise((resolve, reject) => {
    const probe = createServer();
    probe.unref();
    probe.once("error", reject);
    probe.listen(0, host, () => {
      const address = probe.address();
      const port = typeof address === "object" && address ? address.port : null;
      probe.close((error) => {
        if (error) reject(error);
        else if (!port) reject(new Error("Kein freier lokaler Port ermittelbar."));
        else resolve(port);
      });
    });
  });
}

function processHasExited(child) {
  return child.exitCode !== null && child.exitCode !== undefined
    || child.signalCode !== null && child.signalCode !== undefined;
}

async function waitForChildExit(child, timeoutMs) {
  if (processHasExited(child)) return true;
  return new Promise((resolve) => {
    let timer;
    const finish = (exited) => {
      clearTimeout(timer);
      child.off?.("exit", onExit);
      child.off?.("close", onExit);
      resolve(exited);
    };
    const onExit = () => finish(true);
    child.once?.("exit", onExit);
    child.once?.("close", onExit);
    timer = setTimeout(() => finish(false), timeoutMs);
  });
}

export async function terminateChildProcess(child, options = {}) {
  if (!child || processHasExited(child)) return;
  const pid = child.pid;
  if (!Number.isSafeInteger(pid) || pid <= 0) {
    throw new Error("Preview-Prozess kann ohne gültige PID nicht zuverlässig beendet werden.");
  }
  const platform = options.platform ?? process.platform;
  const waitForExit = options.waitForExit ?? waitForChildExit;
  if (platform === "win32") {
    const runTaskkill = options.spawnSyncProcess ?? spawnSync;
    let result;
    try {
      result = runTaskkill("taskkill", ["/pid", String(pid), "/T", "/F"], {
        shell: false,
        stdio: "ignore",
        windowsHide: true,
      });
    } catch (error) {
      result = { error };
    }
    if ((result?.error || result?.status !== 0) && !processHasExited(child)) {
      try { child.kill?.("SIGKILL"); } catch { /* best effort after taskkill failure */ }
    }
    if (!await waitForExit(child, 5_000) && !processHasExited(child)) {
      try { child.kill?.("SIGKILL"); } catch { /* process already ended */ }
      await waitForExit(child, 1_500);
    }
    if (!processHasExited(child) && !await waitForExit(child, 0)) {
      throw new Error(`Prozessbaum ${pid} konnte nicht nachweislich beendet werden.`);
    }
    return;
  }

  const killProcess = options.killProcess ?? process.kill;
  try { killProcess(-pid, "SIGTERM"); }
  catch {
    try { child.kill?.("SIGTERM"); } catch { /* process already ended */ }
  }
  if (await waitForExit(child, 1_500)) return;
  try { killProcess(-pid, "SIGKILL"); }
  catch {
    try { child.kill?.("SIGKILL"); } catch { /* process already ended */ }
  }
  await waitForExit(child, 1_500);
  if (!processHasExited(child) && !await waitForExit(child, 0)) {
    throw new Error(`Prozessbaum ${pid} konnte nicht nachweislich beendet werden.`);
  }
}

function waitForDelayOrLifecycle(delayMs, lifecyclePromise) {
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve({ type: "delay" }), delayMs);
    lifecyclePromise.then((event) => {
      clearTimeout(timer);
      resolve(event);
    });
  });
}

async function probePreview(url, fetchImpl, timeoutMs, externalSignal) {
  const controller = new AbortController();
  const onAbort = () => controller.abort(externalSignal.reason);
  externalSignal?.addEventListener("abort", onAbort, { once: true });
  if (externalSignal?.aborted) onAbort();
  const timer = setTimeout(() => controller.abort(new Error("Readiness-Zeitüberschreitung")), timeoutMs);
  try {
    const response = await fetchImpl(url, { signal: controller.signal });
    return Boolean(response?.ok);
  } catch {
    return false;
  } finally {
    clearTimeout(timer);
    externalSignal?.removeEventListener("abort", onAbort);
  }
}

function errorDetail(error) {
  if (!(error instanceof Error)) return String(error);
  const code = "code" in error && error.code ? ` (${error.code})` : "";
  return `${error.message}${code}`;
}

function asError(error) {
  return error instanceof Error ? error : new Error(String(error));
}

function withPreviewDiagnostics(error, output, prefix = "") {
  const structured = asError(error);
  const diagnostics = output.trim() || "(keine Ausgabe)";
  const baseMessage = prefix ? `${prefix}: ${structured.message}` : structured.message;
  structured.message = `${baseMessage}\nBegrenzte Preview-Ausgabe:\n${diagnostics}`;
  structured.previewDiagnostics = diagnostics;
  return structured;
}

function combinePrimaryAndCleanup(primary, cleanupErrors, message) {
  if (!cleanupErrors.length) return primary;
  return new AggregateError([primary, ...cleanupErrors], `${message}: ${primary.message}`, { cause: primary });
}

async function closeLateBrowser(launchPromise, timeoutMs) {
  let timer;
  const timeout = new Promise((resolve) => {
    timer = setTimeout(() => resolve(null), timeoutMs);
  });
  const lateBrowser = await Promise.race([
    launchPromise.then((candidate) => ({ candidate }), (error) => ({ error })),
    timeout,
  ]);
  clearTimeout(timer);
  if (!lateBrowser) {
    launchPromise.then((candidate) => candidate?.close?.()).catch(() => {});
    return [];
  }
  if (lateBrowser.error) return [asError(lateBrowser.error)];
  try {
    await lateBrowser.candidate?.close?.();
    return [];
  } catch (error) {
    return [asError(error)];
  }
}

export async function startPreviewServer(options = {}) {
  const projectRoot = options.projectRoot ?? PROJECT_ROOT;
  const viteCli = options.viteCli ?? VITE_CLI;
  const timeoutMs = options.timeoutMs ?? 20_000;
  const pollIntervalMs = options.pollIntervalMs ?? 150;
  const fetchImpl = options.fetchImpl ?? fetch;
  const spawnProcess = options.spawnProcess ?? spawn;
  const stopProcess = options.stopProcess ?? terminateChildProcess;
  const now = options.now ?? Date.now;
  const signal = options.signal;
  throwIfAborted(signal);
  const port = options.port ?? await findAvailablePort();
  const baseUrl = `http://127.0.0.1:${port}`;

  if (!fs.existsSync(viteCli) && !options.allowMissingViteCli) {
    throw new Error(`Vite-CLI fehlt: ${viteCli}`);
  }

  let child;
  try {
    child = spawnProcess(process.execPath, [
      viteCli,
      "preview",
      "--host", "127.0.0.1",
      "--port", String(port),
      "--strictPort",
    ], {
      cwd: projectRoot,
      detached: process.platform !== "win32",
      env: process.env,
      shell: false,
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    });
  } catch (error) {
    throw withPreviewDiagnostics(error, "", "Vite-Preview konnte nicht gestartet werden");
  }

  let output = "";
  const onStdout = (chunk) => { output = appendBoundedOutput(output, chunk); };
  const onStderr = (chunk) => { output = appendBoundedOutput(output, chunk); };
  child.stdout?.on("data", onStdout);
  child.stderr?.on("data", onStderr);

  let lifecycleEvent = null;
  let settleLifecycle;
  const lifecyclePromise = new Promise((resolve) => { settleLifecycle = resolve; });
  const onError = (error) => {
    lifecycleEvent = { type: "error", error };
    settleLifecycle(lifecycleEvent);
  };
  const onExit = (code, signal) => {
    lifecycleEvent = { type: "exit", code, signal };
    settleLifecycle(lifecycleEvent);
  };
  const onAbort = () => {
    lifecycleEvent = { type: "abort", error: abortError(signal) };
    settleLifecycle(lifecycleEvent);
  };
  child.once("error", onError);
  child.once("exit", onExit);
  signal?.addEventListener("abort", onAbort, { once: true });
  if (signal?.aborted) onAbort();

  let stopped = false;
  const stop = async () => {
    if (stopped) return;
    stopped = true;
    try { await stopProcess(child); }
    finally {
      child.off?.("error", onError);
      child.off?.("exit", onExit);
      child.stdout?.off?.("data", onStdout);
      child.stderr?.off?.("data", onStderr);
      signal?.removeEventListener("abort", onAbort);
    }
  };
  const assertRunning = () => {
    if (!lifecycleEvent) return;
    if (lifecycleEvent.type === "error") {
      throw withPreviewDiagnostics(lifecycleEvent.error, output, "Vite-Preview-Startfehler");
    }
    if (lifecycleEvent.type === "abort") throw lifecycleEvent.error;
    throw withPreviewDiagnostics(new Error(`Vite-Preview endete vorzeitig (Code ${lifecycleEvent.code ?? "null"}, Signal ${lifecycleEvent.signal ?? "null"}).`), output);
  };

  try {
    const deadline = now() + timeoutMs;
    while (now() < deadline) {
      assertRunning();
      const remaining = Math.max(1, deadline - now());
      const readiness = probePreview(baseUrl, fetchImpl, Math.min(1_000, remaining), signal)
        .then((ready) => ({ type: "probe", ready }));
      const outcome = await Promise.race([readiness, lifecyclePromise]);
      assertRunning();
      if (outcome.type === "probe" && outcome.ready) {
        return { baseUrl, child, diagnostics: () => output, assertRunning, stop };
      }
      const nextRemaining = deadline - now();
      if (nextRemaining <= 0) break;
      await waitForDelayOrLifecycle(Math.min(pollIntervalMs, nextRemaining), lifecyclePromise);
    }
    throw withPreviewDiagnostics(new Error(`Vite-Preview erreichte innerhalb von ${timeoutMs} ms keine Readiness.`), output);
  } catch (error) {
    const primary = asError(error);
    const cleanupErrors = [];
    try { await stop(); } catch (cleanupError) { cleanupErrors.push(asError(cleanupError)); }
    throw combinePrimaryAndCleanup(primary, cleanupErrors, "Preview-Start und Cleanup fehlgeschlagen");
  }
}

export async function withPreview(callback, options = {}) {
  const processSignals = options.signal ? null : createProcessSignalController(options.signalEmitter ?? process);
  const signal = options.signal ?? processSignals.signal;
  let preview;
  let browser;
  let launchPromise;
  let result;
  let primaryError;
  try {
    throwIfAborted(signal);
    preview = await startPreviewServer({ ...options.previewOptions, signal });
    preview.assertRunning();
    const launchBrowser = options.launchBrowser ?? ((launchOptions) => chromium.launch(launchOptions));
    launchPromise = Promise.resolve(launchBrowser({
      executablePath: options.browserExecutable ?? findBrowserExecutable(),
      headless: true,
      args: ["--no-sandbox", "--disable-dev-shm-usage", "--js-flags=--expose-gc"],
    }));
    browser = await raceWithAbort(launchPromise, signal);
    preview.assertRunning();
    result = await raceWithAbort(callback(browser, preview.baseUrl, signal), signal);
  } catch (error) {
    primaryError = preview ? withPreviewDiagnostics(error, preview.diagnostics()) : asError(error);
  }

  const cleanupErrors = [];
  try {
    if (browser) await browser.close();
    else if (launchPromise && primaryError) {
      cleanupErrors.push(...await closeLateBrowser(launchPromise, options.lateBrowserTimeoutMs ?? 1_000));
    }
  } catch (error) {
    cleanupErrors.push(asError(error));
  }
  try {
    if (preview) await preview.stop();
  } catch (error) {
    cleanupErrors.push(asError(error));
  }
  try {
    processSignals?.dispose();
  } catch (error) {
    cleanupErrors.push(asError(error));
  }

  if (primaryError) {
    throw combinePrimaryAndCleanup(primaryError, cleanupErrors, "Browserprüfung und Cleanup fehlgeschlagen");
  }
  if (cleanupErrors.length === 1) throw cleanupErrors[0];
  if (cleanupErrors.length > 1) {
    throw new AggregateError(cleanupErrors, "Mehrere Cleanup-Schritte der Browserprüfung sind fehlgeschlagen.");
  }
  return result;
}
