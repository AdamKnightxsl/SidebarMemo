# AGENTS.md

## Project overview

Tauri 2 desktop app: Vue 3 + TypeScript frontend, Rust backend, SQLite storage. A Windows sidebar memo with global shortcut toggle (Alt+Space), system tray, and acrylic/glass effect. **No tests, no linter, no CI configured.**

## Commands

```bash
# Development (starts Vite on :1420 + Tauri window)
npm run tauri dev

# Type-check frontend only (no emit)
npx vue-tsc --noEmit

# Full frontend build (type-check + vite build)
npm run tauri build
```

There is no `npm test`, `npm run lint`, or `npm run format` script. The build step (`vue-tsc --noEmit`) is the only automated verification.

## Architecture

- **Frontend**: `src/` — Vue 3 SFC, Composition API (`<script setup lang="ts">`)
  - `src/composables/` — shared state (module-level `ref`s, not per-component). `useMemos.ts` and `useSettings.ts` are singletons.
  - `src/views/` — page-level components (`MemoListView`, `SettingsView`)
  - `src/components/` — reusable UI (`MemoCard`, `QuickInput`, `SearchBar`, `SideNav`)
- **Backend**: `src-tauri/src/`
  - `main.rs` — entry, calls `lib::run()`
  - `lib.rs` — Tauri builder, IPC command handlers, tray setup, global shortcut registration
  - `db.rs` — SQLite via `rusqlite` (bundled), `MemoStore` struct
- **IPC bridge**: Frontend calls Rust via `invoke("command_name", { args })`. All commands are registered in `lib.rs:172`.
- **Path alias**: `@` → `./src` (vite + tsconfig)

## Data flow

- Settings stored in `{config_dir}/sidebar-memo/settings.json` (via `dirs::config_dir()`)
- Memo DB at `{data_dir}/sidebar-memo/memos.db` (via `dirs::data_dir()`)
- Composables export module-level singletons — state persists across component mounts. Do not create new instances.

## Conventions

- **Language**: Code comments and UI text are in Chinese (zh-CN)
- **TypeScript**: `strict: true`, but `noUnusedLocals`/`noUnusedParameters` are `false` — unused vars won't error
- **Rust edition**: 2021. Tauri 2 APIs (`WebviewWindow`, `Emitter`, `Manager`). Check `tauri.conf.json` capabilities when adding new IPC commands or plugins.
- **No barrel exports**: components are imported individually, no `index.ts` re-exports

## Gotchas

- Vite dev server is locked to port **1420** (`strictPort: true`). If something is using that port, dev will fail.
- `src-tauri/` is excluded from Vite file watching — Rust changes require restarting `tauri dev`.
- Tauri capabilities (`src-tauri/capabilities/default.json`) control what the frontend can invoke. New plugins or window operations may need permission entries here.
- The `useMemos` composable uses **module-level** `ref`s (not inside the function). Multiple components importing it share the same state — this is intentional.
- Settings shortcut string format: `"Alt+Space"`, `"Ctrl+Shift+A"`, etc. Parsed in `lib.rs:73-109`.
- `TAURI_DEBUG` env var controls Vite sourcemaps and disables minification in dev builds.
