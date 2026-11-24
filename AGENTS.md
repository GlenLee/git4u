# AGENTS.md

## Scope

- This repo currently implements the **M0 vertical slice + M1 increments** (M0: open repo → status/diff/stage/commit → log → branches; M1: settings, credentials, fetch/pull/push, problem drawer, i18n, .gitignore templates). `README.md` is the authoritative status; `docs/14-实现范围与风险基线.md` is the authoritative scope; `docs/` and `prototype/` describe the full product, and most pages/features there are **not implemented**.
- UI copy lives in `app/src/lib/i18n.ts` (`zh` / `en` dictionaries + `t()`); add both languages when adding strings. Backend error text is still Chinese-only.

## Layout and boundaries

- `app/` is the official Tauri root (`src/` + `src-tauri/`). `app/src-tauri` joins the Rust workspace via `workspace = "../../rust"`; there is **no root Cargo.toml**.
- Rust workspace lives in `rust/`: `g4u-model` (IPC data), `g4u-core` (git CLI behind `GitRead`/`GitWrite`), `g4u-cred` (地址级凭证元数据 + OS Keychain), `g4u-ops` (sessions, version, write lock, use cases), `g4u-server` (Tauri commands).
- All writes and Git semantics belong in `g4u-ops` / `g4u-core`. Frontend and `g4u-server` must not shell out to git or touch the filesystem.
- No destructive Git operation (reset/rebase/force push/branch delete/discard) may ship without the P0 safety layer: Precheck + backup ref + Journal + undo mapping. Current code has backup-ref cleanup but no backup creation or Journal.
- Frontend reaches Rust only through `app/src/ipc/index.ts`. No direct `@tauri-apps/api` imports outside `app/src/ipc/`.
- Report errors with `useProblemReporter()` from `app/src/lib/problems.ts` (toast + problem drawer + action binding), not a bare `message.error(errorText(...))`.
- `app/src/ipc/types.ts` is hand-mirrored from `rust/crates/g4u-model/src/lib.rs` (camelCase JSON). Change both when contracts change; there is no codegen.
- Browser mode (`pnpm dev`) uses `app/src/ipc/mock.ts` and fake data. Real Git only works under Tauri.
- Tauri commands must live in `rust/crates/g4u-server/src/commands.rs` (crate-root `pub` commands hit macro E0255). Register every new command in `app/src-tauri/src/lib.rs` `generate_handler!`. JS args are camelCase (`repoId` → `repo_id`).

## Commands

```bash
# Install first; Tauri CLI lives in app/
pnpm --dir app install

# Run
pnpm --dir app tauri dev            # real backend; port 1420 is strictPort
pnpm --dir app dev                  # browser-only preview with mock data

# Focused checks
cargo test --manifest-path rust/Cargo.toml -p g4u-core
cargo test --manifest-path rust/Cargo.toml -p g4u-ops
pnpm --dir app exec vitest run src/lib/fileTree.spec.ts

# Before finishing
cargo fmt --manifest-path rust/Cargo.toml --all
cargo clippy --manifest-path rust/Cargo.toml --workspace --all-targets
cargo test --manifest-path rust/Cargo.toml --workspace
pnpm --dir app test
pnpm --dir app build                # vue-tsc --noEmit + Vite; no separate typecheck script
pnpm --dir app tauri build --debug --no-bundle
```

- There is currently no CI, ESLint, Prettier, or `pnpm lint`. CI + golden-repo differential tests are a **P0 gap** (docs/14), not a settled convention: do not add ESLint/Prettier ad hoc, but do keep `cargo fmt` / `clippy` / `test` and `pnpm build` green. `pnpm build` remains the frontend typecheck gate.
- Rust tests invoke real `git` and create temp repos; `git ≥ 2.34` must be on PATH. No services or fixtures are required.
- `app/pnpm-workspace.yaml` sets `allowBuilds.esbuild`; removing it makes pnpm 11 abort on install.

## Gotchas

- TypeScript is strict with `noUnusedLocals` / `noUnusedParameters`; unused imports fail `pnpm build`.
- Colors/theme come only from `app/src/styles/tokens.ts` (CSS vars + Naive UI overrides). Do not hardcode hex values in components.
- Naive UI components are imported explicitly (no auto-import plugin). Vue SFCs use `<script setup>` and `@/` aliases.
- Both Pinia stores (`useRepoStore`, `useUiStore`) live in `app/src/stores/repo.ts`.
- Refresh is 5s polling + post-operation refresh; there is no filesystem watcher yet. Keep refresh logic in the store rather than adding component timers.
- Settings opens a separate Tauri window (`settings`, route `/settings-window`) from topnav 设置 / Ctrl+,; browser preview falls back to `/settings`. Repo settings (pull/proxy/audit/fetch) persist to `settings.json`; UI preferences stay in `localStorage["git4u.settings"]`; `app_info` supplies the About data.
- Repo list persists to the Tauri app-data dir (`%APPDATA%\dev.git4u.desktop\repos.json` on Windows); delete it to reset the dashboard.
- Generated/ignored: `rust/target`, `app/dist`, `app/src-tauri/gen/schemas`. Do not edit.
- `prototype/` is a static design reference and is not built or served by the app.
- If `tauri dev` reports port 1420 in use, kill the orphaned Vite/node process before retrying.
