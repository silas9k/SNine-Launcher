import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import test from "node:test";
import {
  PROJECT_ROOT,
  VITE_CLI,
  createProcessSignalController,
  startPreviewServer,
  terminateChildProcess,
  withPreview,
} from "../../scripts/browser-utils.mjs";

class FakeChild extends EventEmitter {
  constructor(pid = 4242) {
    super();
    this.pid = pid;
    this.exitCode = null;
    this.signalCode = null;
    this.stdout = new PassThrough();
    this.stderr = new PassThrough();
  }
}

const successfulProbe = async () => ({ ok: true });

test("starts Vite directly through Node and cleans up after readiness", async () => {
  const child = new FakeChild();
  let invocation;
  let stopped = false;
  const preview = await startPreviewServer({
    port: 45101,
    allowMissingViteCli: true,
    fetchImpl: successfulProbe,
    spawnProcess(executable, args, options) {
      invocation = { executable, args, options };
      return child;
    },
    stopProcess: async (received) => { assert.equal(received, child); stopped = true; },
  });
  assert.equal(invocation.executable, process.execPath);
  assert.deepEqual(invocation.args.slice(0, 2), [VITE_CLI, "preview"]);
  assert.equal(invocation.options.cwd, PROJECT_ROOT);
  assert.equal(invocation.options.shell, false);
  await preview.stop();
  assert.equal(stopped, true);
});

test("reports asynchronous spawn errors and cleans up", async () => {
  const child = new FakeChild();
  let stopped = false;
  await assert.rejects(startPreviewServer({
    port: 45102,
    allowMissingViteCli: true,
    fetchImpl: async () => new Promise(() => {}),
    spawnProcess: () => {
      queueMicrotask(() => child.emit("error", Object.assign(new Error("synthetic spawn failure"), { code: "EINVAL" })));
      return child;
    },
    stopProcess: async () => { stopped = true; },
  }), (error) => {
    assert.equal(error.code, "EINVAL");
    assert.match(error.message, /Startfehler: synthetic spawn failure/u);
    return true;
  });
  assert.equal(stopped, true);
});

test("preserves asynchronous EINVAL when default cleanup has no PID", async () => {
  const child = new FakeChild();
  child.pid = undefined;
  let received;
  try {
    await startPreviewServer({
      port: 45108,
      allowMissingViteCli: true,
      fetchImpl: async () => new Promise(() => {}),
      spawnProcess: () => {
        queueMicrotask(() => child.emit("error", Object.assign(new Error("synthetic undefined-pid spawn"), { code: "EINVAL" })));
        return child;
      },
    });
  } catch (error) {
    received = error;
  }
  assert.ok(received instanceof AggregateError);
  assert.equal(received.cause, received.errors[0]);
  assert.equal(received.errors[0].code, "EINVAL");
  assert.match(received.errors[0].message, /synthetic undefined-pid spawn/u);
  assert.match(received.errors[1].message, /ohne gültige PID/u);
  assert.equal(child.listenerCount("error"), 0);
  assert.equal(child.listenerCount("exit"), 0);
  assert.equal(child.stdout.listenerCount("data"), 0);
  assert.equal(child.stderr.listenerCount("data"), 0);
});

test("reports an early preview exit and cleans up", async () => {
  const child = new FakeChild();
  let stopped = false;
  await assert.rejects(startPreviewServer({
    port: 45103,
    allowMissingViteCli: true,
    fetchImpl: async () => new Promise(() => {}),
    spawnProcess: () => {
      queueMicrotask(() => {
        child.exitCode = 2;
        child.emit("exit", 2, null);
      });
      return child;
    },
    stopProcess: async () => { stopped = true; },
  }), /endete vorzeitig \(Code 2/u);
  assert.equal(stopped, true);
});

test("enforces a bounded readiness timeout and cleans up", async () => {
  const child = new FakeChild();
  let stopped = false;
  await assert.rejects(startPreviewServer({
    port: 45104,
    timeoutMs: 0,
    allowMissingViteCli: true,
    spawnProcess: () => child,
    stopProcess: async () => { stopped = true; },
  }), /innerhalb von 0 ms keine Readiness/u);
  assert.equal(stopped, true);
});

test("closes browser and preview after a failed browser test", async () => {
  const child = new FakeChild();
  let browserClosed = false;
  let previewStopped = false;
  await assert.rejects(withPreview(async () => {
    throw new Error("synthetic browser assertion");
  }, {
    browserExecutable: "/synthetic/browser",
    launchBrowser: async () => ({ close: async () => { browserClosed = true; } }),
    previewOptions: {
      port: 45105,
      allowMissingViteCli: true,
      fetchImpl: successfulProbe,
      spawnProcess: () => child,
      stopProcess: async () => { previewStopped = true; },
    },
  }), /synthetic browser assertion/u);
  assert.equal(browserClosed, true);
  assert.equal(previewStopped, true);
});

test("rejects termination without a usable PID", async () => {
  const child = new FakeChild(undefined);
  child.pid = undefined;
  let invoked = false;
  await assert.rejects(terminateChildProcess(child, {
    killProcess: () => { invoked = true; },
    spawnSyncProcess: () => { invoked = true; return { status: 0 }; },
  }), /ohne gültige PID/u);
  assert.equal(invoked, false);
});

test("fails clearly when process termination cannot be confirmed", async () => {
  const child = new FakeChild(5011);
  let killCount = 0;
  await assert.rejects(terminateChildProcess(child, {
    platform: "linux",
    killProcess: () => { killCount += 1; },
    waitForExit: async () => false,
  }), /konnte nicht nachweislich beendet werden/u);
  assert.equal(killCount, 2);
});

test("aborts readiness, stops the child and removes lifecycle listeners", async () => {
  const child = new FakeChild(5012);
  const controller = new AbortController();
  let stopped = false;
  const pendingFetch = (_url, { signal }) => new Promise((_resolve, reject) => {
    signal.addEventListener("abort", () => reject(signal.reason), { once: true });
  });
  const promise = startPreviewServer({
    port: 45106,
    allowMissingViteCli: true,
    signal: controller.signal,
    fetchImpl: pendingFetch,
    spawnProcess: () => child,
    stopProcess: async () => { stopped = true; },
  });
  controller.abort(new Error("synthetic readiness abort"));
  await assert.rejects(promise, /synthetic readiness abort/u);
  assert.equal(stopped, true);
  assert.equal(child.listenerCount("error"), 0);
  assert.equal(child.listenerCount("exit"), 0);
  assert.equal(child.stdout.listenerCount("data"), 0);
  assert.equal(child.stderr.listenerCount("data"), 0);
});

test("SIGTERM aborts a running browser and leaves no signal listeners", async () => {
  const child = new FakeChild(5013);
  const signals = new EventEmitter();
  let browserClosed = false;
  let previewStopped = false;
  const promise = withPreview(async () => {
    signals.emit("SIGTERM");
    await new Promise(() => {});
  }, {
    signalEmitter: signals,
    browserExecutable: "/synthetic/browser",
    launchBrowser: async () => ({ close: async () => { browserClosed = true; } }),
    previewOptions: {
      port: 45107,
      allowMissingViteCli: true,
      fetchImpl: successfulProbe,
      spawnProcess: () => child,
      stopProcess: async () => { previewStopped = true; },
    },
  });
  await assert.rejects(promise, (error) => {
    assert.equal(error.name, "AbortError");
    assert.equal(error.code, "ERR_PREVIEW_ABORTED");
    assert.equal(error.signal, "SIGTERM");
    assert.match(error.message, /SIGTERM/u);
    assert.equal(typeof error.previewDiagnostics, "string");
    return true;
  });
  assert.equal(browserClosed, true);
  assert.equal(previewStopped, true);
  assert.equal(signals.listenerCount("SIGINT"), 0);
  assert.equal(signals.listenerCount("SIGTERM"), 0);
});

test("closes a browser whose launch resolves only after SIGINT abort", async () => {
  const child = new FakeChild(5014);
  const signals = new EventEmitter();
  let browserClosed = false;
  let resolveLaunch;
  const launch = new Promise((resolve) => { resolveLaunch = resolve; });
  const promise = withPreview(async () => {}, {
    signalEmitter: signals,
    browserExecutable: "/synthetic/browser",
    launchBrowser: () => {
      queueMicrotask(() => signals.emit("SIGINT"));
      setTimeout(() => resolveLaunch({ close: async () => { browserClosed = true; } }), 5);
      return launch;
    },
    previewOptions: {
      port: 45109,
      allowMissingViteCli: true,
      fetchImpl: successfulProbe,
      spawnProcess: () => child,
      stopProcess: async () => {},
    },
  });
  await assert.rejects(promise, (error) => {
    assert.equal(error.name, "AbortError");
    assert.equal(error.code, "ERR_PREVIEW_ABORTED");
    assert.equal(error.signal, "SIGINT");
    return true;
  });
  assert.equal(browserClosed, true);
  assert.equal(signals.listenerCount("SIGINT"), 0);
  assert.equal(signals.listenerCount("SIGTERM"), 0);
});

test("aggregates browser and preview cleanup failures behind the primary error", async () => {
  const child = new FakeChild(5015);
  const primary = Object.assign(new Error("primary browser test failure"), { code: "EASSERT" });
  let received;
  try {
    await withPreview(async () => { throw primary; }, {
      browserExecutable: "/synthetic/browser",
      launchBrowser: async () => ({ close: async () => { throw new Error("browser close failure"); } }),
      previewOptions: {
        port: 45110,
        allowMissingViteCli: true,
        fetchImpl: successfulProbe,
        spawnProcess: () => child,
        stopProcess: async () => { throw new Error("preview stop failure"); },
      },
    });
  } catch (error) {
    received = error;
  }
  assert.ok(received instanceof AggregateError);
  assert.equal(received.cause, primary);
  assert.equal(received.errors[0], primary);
  assert.equal(received.errors[0].code, "EASSERT");
  assert.match(received.errors[1].message, /browser close failure/u);
  assert.match(received.errors[2].message, /preview stop failure/u);
});

test("process signal controller removes all listeners without an abort", () => {
  const signals = new EventEmitter();
  const registration = createProcessSignalController(signals);
  assert.equal(signals.listenerCount("SIGINT"), 1);
  assert.equal(signals.listenerCount("SIGTERM"), 1);
  registration.dispose();
  assert.equal(signals.listenerCount("SIGINT"), 0);
  assert.equal(signals.listenerCount("SIGTERM"), 0);
});

test("uses taskkill without a shell for a valid Windows process tree", async () => {
  const child = new FakeChild(5010);
  let invocation;
  await terminateChildProcess(child, {
    platform: "win32",
    spawnSyncProcess(command, args, options) {
      invocation = { command, args, options };
      return { status: 0 };
    },
    waitForExit: async () => true,
  });
  assert.equal(invocation.command, "taskkill");
  assert.deepEqual(invocation.args, ["/pid", "5010", "/T", "/F"]);
  assert.equal(invocation.options.shell, false);
});
