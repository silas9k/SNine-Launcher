import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import { withPreview } from "./browser-utils.mjs";

const require = createRequire(import.meta.url);
const axePath = require.resolve("axe-core/axe.min.js");

const outputRoot = process.env.S9LAB_VISUAL_OUTPUT ? path.resolve(process.env.S9LAB_VISUAL_OUTPUT) : null;
if (outputRoot) fs.mkdirSync(outputRoot, { recursive: true });

const cases = [
  { slug: "900x600-dark-de-compact", viewport: { width: 900, height: 600 }, locale: "de-DE", colorScheme: "dark", appearanceLabel: "Erscheinungsbild", appearance: "Dunkel", densityLabel: "Dichte", density: "Kompakt", navigationLabel: "Navigation", navigation: "Kompakt", motion: "Reduzierte Animationen", reset: "Zurücksetzen", settings: "Einstellungen", library: "Bibliothek", storage: "Speicherübersicht", permanentDeletion: "Dauerhaftes Löschen deaktiviert", home: "Start", task: "Task-Center öffnen", closeTask: "Task-Center schließen" },
  { slug: "1280x720-light-de-comfortable", viewport: { width: 1280, height: 720 }, locale: "de-DE", colorScheme: "light", appearanceLabel: "Erscheinungsbild", appearance: "Hell", densityLabel: "Dichte", density: "Komfortabel", navigationLabel: "Navigation", navigation: "Erweitert", motion: null, reset: "Zurücksetzen", settings: "Einstellungen", library: "Bibliothek", storage: "Speicherübersicht", permanentDeletion: "Dauerhaftes Löschen deaktiviert", home: "Start", task: "Task-Center öffnen", closeTask: "Task-Center schließen" },
  { slug: "1920x1080-contrast-en-comfortable", viewport: { width: 1920, height: 1080 }, locale: "en-US", colorScheme: "dark", appearanceLabel: "Appearance", appearance: "High contrast", densityLabel: "Density", density: "Comfortable", navigationLabel: "Navigation", navigation: "Expanded", motion: null, reset: "Reset", settings: "Settings", library: "Library", storage: "Storage overview", permanentDeletion: "Permanent deletion disabled", home: "Home", task: "Open task center", closeTask: "Close task center" },
  { slug: "1280x720-dark-en-compact", viewport: { width: 1280, height: 720 }, locale: "en-US", colorScheme: "dark", appearanceLabel: "Appearance", appearance: "Dark", densityLabel: "Density", density: "Compact", navigationLabel: "Navigation", navigation: "Compact", motion: null, reset: "Reset", settings: "Settings", library: "Library", storage: "Storage overview", permanentDeletion: "Permanent deletion disabled", home: "Home", task: "Open task center", closeTask: "Close task center" },
  { slug: "640x900-system-en-comfortable", viewport: { width: 640, height: 900 }, locale: "en-US", colorScheme: "light", appearanceLabel: "Appearance", appearance: "System", densityLabel: "Density", density: "Comfortable", navigationLabel: "Navigation", navigation: "Compact", motion: null, reset: "Reset", settings: "Settings", library: "Library", storage: "Storage overview", permanentDeletion: "Permanent deletion disabled", home: "Home", task: "Open task center", closeTask: "Close task center" },
];

const manifest = [];
await withPreview(async (browser, baseUrl) => {
  for (const item of cases) {
    const context = await browser.newContext({ viewport: item.viewport, locale: item.locale, colorScheme: item.colorScheme, reducedMotion: "no-preference" });
    const page = await context.newPage();
    await page.goto(baseUrl, { waitUntil: "networkidle" });
    const navigateTo = async (name) => {
      if (item.viewport.width <= 860) await page.locator(".shell-titlebar__menu").click();
      await page.getByRole("button", { name, exact: true }).click();
    };
    await navigateTo(item.settings);
    await page.getByRole("combobox", { name: item.appearanceLabel }).selectOption({ label: item.appearance });
    await page.getByRole("combobox", { name: item.densityLabel }).selectOption({ label: item.density });
    await page.getByRole("combobox", { name: item.navigationLabel }).selectOption({ label: item.navigation });
    if (item.motion) {
      const motionSwitch = page.getByRole("switch", { name: item.motion });
      await motionSwitch.focus();
      await page.keyboard.press("Space");
      if (!(await motionSwitch.isChecked())) throw new Error(`${item.slug}: reduced-motion switch did not respond to keyboard input`);
      const reduced = await page.evaluate(() => ({ attribute: document.documentElement.dataset.reducedMotion, transition: getComputedStyle(document.querySelector(".shell-nav__item")).transitionDuration }));
      if (reduced.attribute !== "true" || reduced.transition !== "0s") throw new Error(`${item.slug}: reduced motion did not disable transitions`);
    }
    const reset = page.getByRole("button", { name: item.reset, exact: true });
    await reset.click();
    const dialog = page.getByRole("dialog");
    await dialog.waitFor({ state: "visible" });
    await page.waitForFunction(() => Boolean(document.querySelector('.ui-dialog')?.contains(document.activeElement)), undefined, { timeout: 2_000 }).catch(() => { throw new Error(`${item.slug}: dialog did not receive focus`); });
    await page.keyboard.press("Escape");
    await dialog.waitFor({ state: "hidden" });
    await navigateTo(item.library);
    await page.getByRole("heading", { name: item.storage, exact: true }).waitFor({ state: "visible" });
    await page.getByText(item.permanentDeletion, { exact: true }).waitFor({ state: "visible" });
    const libraryLayout = await page.evaluate(() => {
      const pageElement = document.querySelector(".library-page");
      const storage = document.querySelector(".storage-overview");
      if (!pageElement || !storage) return null;
      const pageRect = pageElement.getBoundingClientRect();
      const storageRect = storage.getBoundingClientRect();
      return {
        withinPage: storageRect.left >= pageRect.left - 1 && storageRect.right <= pageRect.right + 1,
        documentWidth: document.documentElement.scrollWidth,
        viewportWidth: window.innerWidth,
      };
    });
    if (!libraryLayout?.withinPage || libraryLayout.documentWidth > libraryLayout.viewportWidth + 1) {
      throw new Error(`${item.slug}: Phase-4 library layout overflow ${JSON.stringify(libraryLayout)}`);
    }
    await page.addScriptTag({ path: axePath });
    const libraryViolations = await page.evaluate(async () => {
      const results = await globalThis.axe.run(document, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21aa"] } });
      return results.violations.filter((violation) => violation.impact === "serious" || violation.impact === "critical").map((violation) => ({ id: violation.id, targets: violation.nodes.map((node) => node.target) }));
    });
    if (libraryViolations.length) throw new Error(`${item.slug}: Phase-4 library accessibility violations ${JSON.stringify(libraryViolations)}`);
    await navigateTo(item.home);
    const preview = await page.evaluate(() => {
      const stage = document.querySelector('[data-preview-surface="integrated"]');
      const viewport = document.querySelector(".player-stage__viewport");
      const player = document.querySelector(".player-render-anchor");
      const profiles = document.querySelector(".home-profiles");
      const status = document.querySelector(".home-status");
      if (!stage || !viewport || !player || !profiles || !status) return null;
      const styleSummary = (element) => {
        const style = getComputedStyle(element);
        return {
          backgroundColor: style.backgroundColor,
          backgroundImage: style.backgroundImage,
          borderWidths: [style.borderTopWidth, style.borderRightWidth, style.borderBottomWidth, style.borderLeftWidth],
          borderRadius: style.borderRadius,
          boxShadow: style.boxShadow,
        };
      };
      const rect = (element) => {
        const value = element.getBoundingClientRect();
        return { left: value.left, top: value.top, right: value.right, bottom: value.bottom, width: value.width, height: value.height };
      };
      const overlaps = (first, second) => first.left < second.right && first.right > second.left && first.top < second.bottom && first.bottom > second.top;
      const stageRect = rect(stage);
      const viewportRect = rect(viewport);
      const playerRect = rect(player);
      const profilesRect = rect(profiles);
      const statusRect = rect(status);
      return {
        tagName: stage.tagName,
        className: stage.className,
        stage: styleSummary(stage),
        viewport: styleSummary(viewport),
        playerInsideViewport: playerRect.left >= viewportRect.left - 1 && playerRect.right <= viewportRect.right + 1 && playerRect.top >= viewportRect.top - 1 && playerRect.bottom <= viewportRect.bottom + 1,
        playerInsideStage: playerRect.left >= stageRect.left - 1 && playerRect.right <= stageRect.right + 1 && playerRect.top >= stageRect.top - 1 && playerRect.bottom <= stageRect.bottom + 1,
        overlapsSidePanel: overlaps(playerRect, profilesRect) || overlaps(playerRect, statusRect),
      };
    });
    if (!preview) throw new Error(`${item.slug}: integrated player preview structure is missing`);
    const isTransparent = (color) => color === "transparent" || /^rgba\([^)]*,\s*0\)$/u.test(color);
    const isBoxless = (surface) => isTransparent(surface.backgroundColor)
      && surface.backgroundImage === "none"
      && surface.borderWidths.every((width) => width === "0px")
      && surface.borderRadius === "0px"
      && surface.boxShadow === "none";
    if (preview.tagName !== "SECTION" || preview.className.includes("ui-card") || !isBoxless(preview.stage) || !isBoxless(preview.viewport)) {
      throw new Error(`${item.slug}: player preview rendered as a card or bounded panel ${JSON.stringify(preview)}`);
    }
    if (!preview.playerInsideViewport || !preview.playerInsideStage || preview.overlapsSidePanel) {
      throw new Error(`${item.slug}: player preview is clipped or overlaps another home region ${JSON.stringify(preview)}`);
    }
    await page.getByRole("button", { name: item.task, exact: true }).click();
    const drawer = page.locator(".task-center--open");
    await drawer.waitFor({ state: "visible" });
    await page.waitForFunction(({ width, height }) => {
      const openDrawer = document.querySelector(".task-center--open");
      if (!openDrawer) return false;
      const rect = openDrawer.getBoundingClientRect();
      return rect.left >= -1 && rect.top >= -1 && rect.right <= width + 1 && rect.bottom <= height + 1;
    }, item.viewport, { timeout: 2_000 }).catch(() => {
      throw new Error(`${item.slug}: task center did not settle inside the viewport`);
    });
    const drawerBox = await drawer.boundingBox();
    if (!drawerBox || drawerBox.x < -1 || drawerBox.y < -1 || drawerBox.x + drawerBox.width > item.viewport.width + 1 || drawerBox.y + drawerBox.height > item.viewport.height + 1) throw new Error(`${item.slug}: task center is outside the viewport`);
    await page.waitForFunction(() => Boolean(document.querySelector(".task-center--open")?.contains(document.activeElement)), undefined, { timeout: 2_000 }).catch(() => { throw new Error(`${item.slug}: task center did not receive focus`); });
    await page.keyboard.press("Escape");
    await drawer.waitFor({ state: "hidden" });
    await page.waitForFunction(() => {
      const panel = document.querySelector(".task-center");
      if (!panel) return false;
      const style = getComputedStyle(panel);
      return panel.getAttribute("aria-hidden") === "true"
        && style.visibility === "hidden"
        && !document.querySelector(".task-center__scrim");
    }, undefined, { timeout: 2_000 }).catch(() => {
      throw new Error(`${item.slug}: task center close transition did not settle`);
    });

    const overflow = await page.evaluate(() => {
      const visible = Array.from(document.querySelectorAll("body *")).filter((element) => {
        const style = getComputedStyle(element);
        const rect = element.getBoundingClientRect();
        return style.display !== "none" && style.visibility !== "hidden" && rect.width > 0 && rect.height > 0;
      });
      return visible.filter((element) => element.scrollWidth > element.clientWidth + 2 && getComputedStyle(element).overflowX === "visible").map((element) => `${element.tagName}.${element.className}`).slice(0, 20);
    });
    if (overflow.length) throw new Error(`${item.slug}: horizontal overflow: ${overflow.join(", ")}`);

    const bodyWidth = await page.evaluate(() => ({ body: document.body.scrollWidth, root: document.documentElement.scrollWidth, viewport: window.innerWidth, language: document.documentElement.lang, title: document.title }));
    if (bodyWidth.body > bodyWidth.viewport + 1 || bodyWidth.root > bodyWidth.viewport + 1) throw new Error(`${item.slug}: document overflow ${JSON.stringify(bodyWidth)}`);
    const expectedLanguage = item.locale.startsWith("de") ? "de" : "en";
    if (bodyWidth.language !== expectedLanguage || bodyWidth.title !== "S9Lab") throw new Error(`${item.slug}: document language/title mismatch ${JSON.stringify(bodyWidth)}`);

    await page.addScriptTag({ path: axePath });
    const violations = await page.evaluate(async () => {
      const results = await globalThis.axe.run(document, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21aa"] } });
      return results.violations.filter((violation) => violation.impact === "serious" || violation.impact === "critical").map((violation) => ({ id: violation.id, impact: violation.impact, targets: violation.nodes.map((node) => node.target) }));
    });
    if (violations.length) throw new Error(`${item.slug}: accessibility violations ${JSON.stringify(violations)}`);

    await page.evaluate(() => { if (document.activeElement instanceof HTMLElement) document.activeElement.blur(); });
    await page.mouse.move(Math.floor(item.viewport.width / 2), 2);
    if (outputRoot) {
      const file = path.join(outputRoot, `${item.slug}.png`);
      const buffer = await page.screenshot({ path: file, fullPage: false, animations: "disabled", caret: "hide" });
      if (buffer.length < 20_000) throw new Error(`${item.slug}: screenshot unexpectedly small`);
      manifest.push({ file: path.basename(file), width: item.viewport.width, height: item.viewport.height, sha256: createHash("sha256").update(buffer).digest("hex") });
    }
    await context.close();
  }
});
if (outputRoot) fs.writeFileSync(path.join(outputRoot, "visual-manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`Phase-2 browser checks passed for ${cases.length} responsive/theme/locale cases including the boxless player preview.`);
