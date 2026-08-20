# Phase 3: geänderte und neue Dateien

## Neu

- `docs/PHASE-3-ARCHITECTURE.md`
- `docs/PHASE-3-CHANGED-FILES.md`
- `docs/PHASE-3-MIGRATIONS.md`
- `docs/PHASE-3-SECURITY.md`
- `docs/PHASE-3-TEST-MATRIX.md`
- `scripts/check-phase3-auth-security.mjs`
- `src/lib/authCommands.ts`
- `src/pages/AccountsPage.tsx`
- `src-tauri/src/auth/service.rs`
- `tests/node/phase3-auth-security.test.mjs`
- `tests/unit/accounts.test.tsx`

## Geändert

- `contracts/ipc-contracts.json`
- `package.json`
- `scripts/check-ipc-contracts.mjs`
- `src/App.tsx`
- `src/i18n/messages.ts`
- `src/lib/commands.ts`
- `src/lib/generated/ipc-contracts.ts`
- `src/pages/PlaceholderPage.tsx`
- `src/styles/layout.css`
- `src/types.ts`
- `src-tauri/src/auth/microsoft.rs`
- `src-tauri/src/auth/mod.rs`
- `src-tauri/src/auth/model.rs`
- `src-tauri/src/auth/store.rs`
- `src-tauri/src/commands.rs`
- `src-tauri/src/ipc/mod.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/logging.rs`
- `src-tauri/src/minecraft/launcher.rs`
- `src-tauri/src/storage/migrations.rs`
- `src-tauri/src/storage/mod.rs`
- `src-tauri/src/storage/models.rs`

## Gelöscht

Keine Datei wurde gelöscht. Aus bestehenden TypeScript-Dateien wurden ausschließlich veraltete öffentliche Token-/Coins-/Friends-/Cosmetic-Platzhaltertypen und die unsicheren alten Auth-Wrapper entfernt.

`Cargo.toml`, `Cargo.lock`, `package-lock.json`, Produktversion, Phase-2-Spielervorschau, Assets und visuelle Basistestfälle blieben unverändert.

