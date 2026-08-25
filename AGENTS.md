# S9Lab Launcher Agent Guidance

## Mission

Build and maintain a secure, stable Minecraft launcher experience with:

- verified runtime installation and rollback safety
- reproducible content profiles
- safe content installs with strict dependency and target checks
- a default, NoRisk-inspired recommendation layer for new profiles

## Operating principles

1. Prefer secure, fail-closed validation over permissive guesses.
2. Keep profile state atomic: aborts must leave the previous state active.
3. Default recommendations are informational and safe by default; they must never bypass dependency or compatibility validation.
4. New profile features should be reversible and reproducible.
5. Do not add broad, unverified content-install behavior without a compatibility gate.

## Project commands

- One-click project start: `npm run start:app`
- Frontend: `npm install`
- Dev UI: `npm run dev`
- Desktop app: `npm run tauri:dev`
- Test frontend: `npm run test:unit`
- Type check + build: `npm run build`
- Rust runtime validation: `cd src-tauri && cargo test minecraft::service -- --nocapture`

## Automation defaults

- Keep the normal dev launch via `START-DEV.ps1` as the single auto-start entry point.
- Prefer the workspace task `Launch SNine Launcher (dev)` when opening the repo in VS Code.
- Treat app startup, validation, and content-install checks as part of the default developer loop.

## Focus areas for this repo

- UI flows in `src/components`
- launcher/profile logic in `src/lib`
- runtime and content integrity in `src-tauri/src`
- security validation of paths, hashes, and profile resolution is the highest priority

## Default-mod recommendation policy

- Show recommendations only for known compatible loaders and Minecraft versions.
- Keep the recommendation list conservative and clearly labeled as defaults.
- The list must be treated as a suggestion layer, not a bypass around dependency resolution.
- Respect the active profile runtime when choosing the loader-specific bundle.

## Delivery standard

Before considering work complete:

- verify the affected test target runs without regressions
- confirm the feature is safe under the current profile and runtime constraints
- keep the profile state transaction-safe
- prefer narrow fixes over large speculative changes
