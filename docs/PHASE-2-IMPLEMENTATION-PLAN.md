# Phase 2 implementation plan

## Scope

Phase 2 replaces the legacy frontend presentation with one S9Lab design system, a responsive shell, typed German/English internationalization, accessible reusable controls, persistent shell preferences through Rust IPC, browser-level layout checks and a reproducible performance harness. Phase-1 storage, operations, path security, download and cache code remains unchanged except for the typed settings IPC integration.

## File groups

- `src/app/`: shell composition, routing, error boundary and state.
- `src/components/ui/`: reusable buttons, fields, menus, dialogs, status, feedback and task-center components.
- `src/components/shell/`: title bar, primary/secondary navigation and responsive shell regions.
- `src/pages/`: honest Phase-2 pages and the three-column start layout.
- `src/i18n/`: typed dictionaries, interpolation, plural rules and Intl helpers.
- `src/theme/`: theme application and accent palette validation.
- `src/styles/`: the only production CSS entry point, semantic tokens and component/layout rules.
- `src-tauri/src/app/config.rs`: settings schema v2, validation, migration and atomic persistence.
- `src-tauri/src/ipc/mod.rs`: typed shell bootstrap/save commands and stable errors.
- `contracts/ipc-contracts.json`: shared Phase-1/Phase-2 IPC contract source.
- `scripts/`: i18n, raw-text, CSS/token, accessibility/layout and performance checks.
- `tests/`: component and browser tests; no test harness is routed from the production app.

## Persistent shell settings

`settings.json` remains the source of truth. Phase 2 adds: `appearance`, `locale`, `navigation_mode`, `background_variant`, and keeps `accent_color`, `ui_density`, and `reduced_motion`. The frontend receives and saves them only via typed IPC. Browser tests use a non-persistent in-memory adapter when Tauri IPC is unavailable; production never uses localStorage.

## IPC

- `phase2_shell_bootstrap` -> `{ settings }`
- `phase2_save_shell_settings` <- `{ settings }` -> `{ settings }`

The shared contract generates TypeScript interfaces and is checked against Rust constants and registered handlers.

## Test plan

1. Static checks: UTF-8, secrets, CSP, contract generation, translation parity, raw visible text, external imports and uncontrolled colors.
2. Unit/component tests: interpolation/plurals, locale fallback, contrast palette, theme application, keyboard navigation, dialog focus restoration, labels and reduced motion.
3. Browser tests: mandatory viewports, themes, locales, density modes, no horizontal overflow, task center/dialog reachability and screenshots.
4. Performance harness: shell-ready mark, navigation responsiveness, 100 page changes and browser heap delta. Windows Tauri process-memory measurement remains a separate script because browser metrics are not equivalent to native WebView process memory.
5. Full Phase-1 Rust regression and Tauri/NSIS build in Windows CI.

## Delivery gate

Phase 2 is complete only after the frontend checks and browser suite are green, the source package is clean, all required documents and visual captures are produced, and Windows commands are provided for the Rust/Tauri checks that cannot run in the current environment.
