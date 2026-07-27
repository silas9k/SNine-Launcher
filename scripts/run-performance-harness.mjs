import fs from "node:fs";
import path from "node:path";
import { withPreview } from "./browser-utils.mjs";

const output = path.resolve(process.env.S9LAB_PERFORMANCE_OUTPUT ?? "artifacts/phase2-performance-browser.json");
fs.mkdirSync(path.dirname(output), { recursive: true });

const report = await withPreview(async (browser, baseUrl) => {
  const context = await browser.newContext({ viewport: { width: 1280, height: 720 }, locale: "en-US", colorScheme: "dark" });
  const page = await context.newPage();
  const session = await context.newCDPSession(page);
  await session.send("Performance.enable");
  const navigationStart = performance.now();
  await page.goto(baseUrl, { waitUntil: "networkidle" });
  await page.waitForFunction(() => performance.getEntriesByName("s9lab.shell.ready").length > 0);
  const shellMetrics = await page.evaluate(() => ({
    startToReadyMs: performance.getEntriesByName("s9lab.app.start-to-shell-ready")[0]?.duration ?? null,
    interactiveMarkMs: performance.getEntriesByName("s9lab.shell.interactive")[0]?.startTime ?? null,
  }));
  await page.evaluate(() => { if (typeof globalThis.gc === "function") globalThis.gc(); });
  const beforeMetrics = await session.send("Performance.getMetrics");
  const getMetric = (metrics, name) => metrics.metrics.find((metric) => metric.name === name)?.value ?? 0;
  const beforeHeap = getMetric(beforeMetrics, "JSHeapUsedSize");

  for (let index = 0; index < 100; index += 1) {
    const name = index % 2 === 0 ? "Library" : "Home";
    await page.getByRole("button", { name, exact: true }).click();
  }
  await page.evaluate(() => { if (typeof globalThis.gc === "function") globalThis.gc(); });
  const afterMetrics = await session.send("Performance.getMetrics");
  const afterHeap = getMetric(afterMetrics, "JSHeapUsedSize");
  const navigationDurations = await page.evaluate(() => performance.getEntriesByName("s9lab.navigation").map((entry) => entry.duration));
  navigationDurations.sort((a, b) => a - b);
  const p95 = navigationDurations[Math.max(0, Math.ceil(navigationDurations.length * 0.95) - 1)] ?? null;
  const result = {
    measuredAt: new Date().toISOString(),
    environment: "Chromium browser harness; not equivalent to native Tauri cold-start or process working set",
    harnessWallTimeMs: performance.now() - navigationStart,
    shell: shellMetrics,
    navigation: { samples: navigationDurations.length, p95Ms: p95, maxMs: navigationDurations.at(-1) ?? null },
    memory: { jsHeapBeforeMiB: beforeHeap / 1024 / 1024, jsHeapAfterMiB: afterHeap / 1024 / 1024, retainedDeltaMiB: (afterHeap - beforeHeap) / 1024 / 1024 },
    targets: { shellReadyMs: 3000, navigationResponseMs: 100, retainedDeltaMiB: 30 },
  };
  await context.close();
  return result;
});
fs.writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`);
if (report.shell.startToReadyMs != null && report.shell.startToReadyMs > report.targets.shellReadyMs) throw new Error(`Browser shell-ready target missed: ${report.shell.startToReadyMs}ms`);
if (report.navigation.p95Ms != null && report.navigation.p95Ms > report.targets.navigationResponseMs) throw new Error(`Navigation p95 target missed: ${report.navigation.p95Ms}ms`);
if (report.memory.retainedDeltaMiB > report.targets.retainedDeltaMiB) throw new Error(`Browser retained heap target missed: ${report.memory.retainedDeltaMiB}MiB`);
console.log(`Performance harness completed: ${output}`);
