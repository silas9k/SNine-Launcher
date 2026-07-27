# Phase 2 – Geänderte und entfernte Dateien

> Historische Liste für den Übergang von Phase 1 v1.0.3 auf Phase 2 v1.0. Die Korrekturdifferenz von Phase 2 v1.0 auf v1.0.1 steht in `PHASE-2-v1.0.1-CHANGED-FILES.md`.

Vergleichsgrundlage: freigegebenes Phase-1-v1.0.3-Quellpaket. Generierte Buildordner sind ausgeschlossen.

## Neu (51)

- `MEASURE-PHASE2-WINDOWS.ps1`
- `VERIFY-PHASE2-WINDOWS.ps1`
- `docs/PHASE-1-WINDOWS-VERIFICATION-ADDENDUM.md`
- `docs/PHASE-2-ACCESSIBILITY.md`
- `docs/PHASE-2-ARCHITECTURE.md`
- `docs/PHASE-2-CHANGED-FILES.md`
- `docs/PHASE-2-COMPLETION-REPORT.md`
- `docs/PHASE-2-DESIGN-SYSTEM.md`
- `docs/PHASE-2-I18N.md`
- `docs/PHASE-2-IMPLEMENTATION-PLAN.md`
- `docs/PHASE-2-NAVIGATION-AND-PAGES.md`
- `docs/PHASE-2-PERFORMANCE.md`
- `docs/PHASE-2-TEST-MATRIX.md`
- `docs/PHASE-2-WINDOWS-VERIFICATION.md`
- `scripts/browser-utils.mjs`
- `scripts/check-design-system.mjs`
- `scripts/check-i18n.mjs`
- `scripts/check-visible-text.mjs`
- `scripts/run-performance-harness.mjs`
- `scripts/run-phase2-browser-tests.mjs`
- `src/app/ErrorBoundary.tsx`
- `src/app/shellStore.ts`
- `src/components/shell/Navigation.tsx`
- `src/components/shell/TaskCenter.tsx`
- `src/components/shell/TitleBar.tsx`
- `src/components/shell/Toasts.tsx`
- `src/components/ui/index.tsx`
- `src/i18n/I18nProvider.tsx`
- `src/i18n/messages.ts`
- `src/lib/shellCommands.ts`
- `src/pages/HomePage.tsx`
- `src/pages/PlaceholderPage.tsx`
- `src/pages/SettingsPage.tsx`
- `src/styles/base.css`
- `src/styles/components.css`
- `src/styles/index.css`
- `src/styles/layout.css`
- `src/styles/tokens.css`
- `src/theme/accent.ts`
- `src/theme/applyTheme.ts`
- `src/theme/types.ts`
- `tests/setup.ts`
- `tests/unit/accent.test.ts`
- `tests/unit/accessibility.test.tsx`
- `tests/unit/dialog.test.tsx`
- `tests/unit/i18n.test.ts`
- `tests/unit/keyboard-components.test.tsx`
- `tests/unit/navigation.test.tsx`
- `tests/unit/theme.test.ts`
- `vitest.config.ts`

## Geändert (18)

- `.github/workflows/phase1-windows-verification.yml`
- `README.md`
- `contracts/ipc-contracts.json`
- `index.html`
- `package-lock.json`
- `package.json`
- `scripts/check-ipc-contracts.mjs`
- `scripts/check-security-config.mjs`
- `scripts/generate-ipc-contracts.mjs`
- `src-tauri/src/app/config.rs`
- `src-tauri/src/ipc/mod.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/platform/mod.rs`
- `src-tauri/tauri.conf.json`
- `src/App.tsx`
- `src/lib/generated/ipc-contracts.ts`
- `src/main.tsx`
- `vite.config.ts`

## Entfernt (31)

- `src/assets/default-skin.png`
- `src/atlas.css`
- `src/components/CapeBackPreview.tsx`
- `src/components/FullScreenInstaller.tsx`
- `src/components/JavaProgressBar.tsx`
- `src/components/LauncherEnhancements.tsx`
- `src/components/Logo.tsx`
- `src/components/Modal.tsx`
- `src/components/NewsPanel.tsx`
- `src/components/PlayerSkin.tsx`
- `src/components/Sidebar.tsx`
- `src/components/TitleBar.tsx`
- `src/config/content.ts`
- `src/enhancements.css`
- `src/hooks/useJavaProgress.ts`
- `src/launcher-theme.css`
- `src/lib/designProfiles.ts`
- `src/lib/news.ts`
- `src/lib/outfits.ts`
- `src/minecraft-client-ui.css`
- `src/pages/Accounts.tsx`
- `src/pages/ClientPage.tsx`
- `src/pages/Downloads.tsx`
- `src/pages/Home.tsx`
- `src/pages/Logs.tsx`
- `src/pages/News.tsx`
- `src/pages/Settings.tsx`
- `src/premium-overhaul.css`
- `src/s9lab-next.css`
- `src/store/launcherStore.ts`
- `src/styles.css`

## Einordnung

Entfernt wurden die parallelen Legacy-Stylesheets, alten Seiten, alten Shell-Komponenten und nicht mehr verwendeten Vorschau-/Store-Implementierungen. Der freigegebene Phase-1-Kern für SQLite, Operationen, Pfadsicherheit, Downloads und Cache bleibt erhalten.

Die Rust-Änderungen beschränken sich auf validierte Shell-Einstellungen, atomare Einstellungsspeicherung, die Plattformgrenze und die neuen gemeinsam typisierten Phase-2-IPC-Commands. Spätere Fachfunktionen wurden nicht vorgezogen.
