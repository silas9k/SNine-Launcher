# Continuation plan for the launcher

## Goal

Keep the project stable, startable, and aligned with the intended NoRisk-style default experience without bypassing safety checks.

## Current status

- Launcher dev flow is working through START-DEV.ps1 and the VS Code task setup.
- Default-mod recommendation list is implemented in a reusable module and displayed in the discover/editor flow.
- Verified install flow remains the only path for recommended content installs.
- Security-sensitive runtime/content checks remain fail-closed and unchanged.

## Next workstream

1. Consolidate reusable recommendation data and ensure every profile loader maps to a safe default bundle.
2. Expand the recommendation catalog for Vanilla and NeoForge with conservative starter sets.
3. Keep startup automation friction-free: the repository starter task and the npm start alias are the canonical entry points.
4. Add or update tests whenever recommendation data or installer behavior changes.
5. Keep the UI labels and status messages clear and consistent.

## Guardrails

- Never treat recommendations as a bypass around dependency validation.
- Only show default suggestions for known compatible versions/loaders.
- Prefer small, curated bundles over broad content injection.
- Every change must be validated with the content editor regression test.

## Immediate next execution

- Run the content editor regression after library consolidation.
- If the default bundle needs expansion, add only verified-safe entries.
- Keep the app in the standard dev loop rather than switching to ad-hoc commands.
