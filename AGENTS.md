# AGENTS.md

## What this repo is

Fork of [CyberTimon/RapidRAW](https://github.com/CyberTimon/RapidRAW) — a GPU-accelerated, non-destructive RAW editor — being ported to HarmonyOS/OpenHarmony. Tauri v2 app: React 19 + TypeScript frontend in `src/`, Rust backend in `src-tauri/`.

- `main` mirrors upstream RapidRAW (Windows/macOS/Linux + Android); `feature/harmonyos-port` is the port work.
- **`docs/HARMONYOS_PORTING.md` (Chinese, this branch's `docs/`) is the authoritative porting doc**: phase progress, verified command sequences, risk register. Read it before touching port code; do not reconstruct its recipes from memory.
- Port strategy: Tauri community `feat/open-harmony` branch wired in via `[patch.crates-io]` in `src-tauri/Cargo.toml`. While patches are in place, **all platforms build against the fork** (tauri 2.11.5, not upstream 2.12).

## Commands

Frontend (repo root):

- `npm start` — full app (`tauri dev`); `npm run dev` — frontend only (Vite, port 1420, strictPort)
- `npm run typecheck` / `lint` / `lint:fix` / `format` / `format:check`
- `npm run tauri build` — release build; `npm run start:tethering` — dev with the `tethering` feature (macOS/Linux only, needs `libgphoto2`)
- i18n (14 locales): `npm run i18n:extract` — extract new keys; `npm run i18n:check` — sync + runtime check

Rust — always in `src-tauri/` (no workspace; `rust-toolchain.toml` pinning Rust 1.98 lives there, not at repo root):

- CI hard gates, exact commands: `cargo fmt -p RapidRAW -- --check` and `cargo clippy --all-targets --all-features -- -D warnings`

## No test suite

No frontend tests, no Rust unit tests. Verification = lint + typecheck + clippy + fmt check + `i18n:check`, then manually exercising the affected surface in the running app. UI smoothness changes: scripted replay in `bench/` (see `bench/README.md`; only same-machine scripted-vs-scripted runs are comparable).

## Conventions & gotchas

- **All user-visible strings go through i18next** (`t()`); no literal JSX text — locale JSONs in `src/i18n/locales/` must stay in sync or `i18n:check` fails.
- **First Rust build downloads ONNX Runtime**: `src-tauri/build.rs` fetches it from HuggingFace, sha256-verifies it into `src-tauri/resources/` (gitignored, cached). Builds need network the first time; on CN networks seed `onnxruntime.dll` there from `hf-mirror.com` (same URL path) — build.rs verifies whatever is on disk and skips the download.
- **Runtime AI models** (SAM/U-2Net/DepthAnything/CLIP/LaMa) download from `hf-mirror.com` into `app_data_dir()/models/` — no seeding needed. On OHOS the app_data_dir is a FUSE mount that denies `rename()` (EACCES): `persist_downloaded_asset` in `ai_processing.rs` must keep its plain byte-copy fallback.
- `src-tauri/src/lib.rs` is the monolith hub registering ~127 Tauri IPC commands; per-feature modules sit beside it (`gpu_processing.rs`, `raw_processing.rs`, `file_management.rs`, `android_integration.rs`, `ohos_integration.rs`, …) with heavy `cfg` platform gating (Rust edition 2024).
- Frontend renders via Canvas 2D / Konva — **no WebGL/WebGPU**. GPU work is Rust-side wgpu: direct surface rendering (desktop) or compute + readback + IPC bitmap (Android/Linux/OHOS).
- State lives in zustand stores (`src/store/use*Store.ts`); `src/App.tsx` wires everything — views in `src/components/views/`, panels in `src/components/panel/`, custom titlebar in `src/window/`.
- Main window is frameless/transparent, created at runtime (`"create": false` in `tauri.conf.json`). Tauri v2 capabilities in `src-tauri/capabilities/`.
- `src-tauri/gen/` (except `gen/android/`) and `src-tauri/libs/` are generated/local artifacts — gitignored, never edit.

## HarmonyOS port rules (`feature/harmonyos-port`)

Violating any of these breaks builds — details in `docs/HARMONYOS_PORTING.md` §6. New OHOS integration code belongs in `src-tauri/src/ohos_integration.rs`.

- **The OHOS target satisfies `target_os = "linux"`** (with `target_env = "ohos"`), so bare `target_os = "linux"` / `any(windows, linux)` gates pull GTK/webkit2gtk into OHOS builds. Established cfg vocabulary: OHOS `target_env = "ohos"`; mobile `any(target_os = "android", target_env = "ohos")`; desktop Linux `all(target_os = "linux", not(target_env = "ohos"))`; desktop trio `any(windows, macos, all(linux, not(ohos)))`.
- **Never run bare `cargo update`.** Cargo.lock is load-bearing: the archmage trio (archmage 0.9.28 + archmage-macros 0.9.28 + magetypes 0.9.26) breaks all aarch64 builds if macros drifts, and `windows` is hand-pinned to 0.62.2 (gpu-allocator vs wgpu-hal conflict). Update with explicit package lists, then re-verify host and `aarch64-unknown-linux-ohos` checks.
- **Cross-compile env vars are shell-only** (`ORT_SKIP_DOWNLOAD`, `CMAKE_TOOLCHAIN_FILE`, `CMAKE_GENERATOR`, `OHOS_NDK_HOME`, plus `CC`/`CXX`/`AR` suffixed with the full target triple — cc-rs needs them; PATH wrappers alone are not enough). Never commit them to `.cargo/config.toml` — cargo `[env]` is global and poisons host builds. Only per-target linker/CC wrappers go in `src-tauri/.cargo/config.toml` (gitignored, machine-local; fresh clones have none — setup: doc §6.1/§8).
- Focused port verification: with the §6.1 shell env set, both `cargo check` (host) and `cargo check --target aarch64-unknown-linux-ohos` in `src-tauri/` must pass.
- `OHOS_HOME`/`OHOS_NDK_HOME` must be the SDK root (`.../sdk/default/openharmony`), not the `native` dir. On Windows hosts, SDK paths must be space-free (junction alias) or cc-rs splits HMS includes and aws-lc-sys/ring fail to compile.
- OHOS builds do **not** download ONNX Runtime; each machine provides `src-tauri/libs/ohos/<abi>/libonnxruntime.so` (gitignored). **`ohrs build` wipes `gen/ohos/entry/libs/<abi>/`**, so ORT + the NDK `libc++_shared.so` (ORT's DT_NEEDED; cmake only ships arm64) must be injected into the unsigned HAP after packaging — `scripts/build-ohos.ps1` (in-repo) automates this. `ort` is load-dynamic; AI features degrade gracefully when missing.
- `src-tauri/vendor/openharmony-ability/` is a vendored pin (upstream master dropped the `webview` feature the wry fork needs) — don't update casually; unwind notes sit above the patch table in `Cargo.toml`.
- `@tauri-apps/api` is pinned to 2.11 in `package.json` to match the Rust fork — keep both in the same major.minor. OHOS commands go through the cargo-installed tauri-cli fork (`cargo install tauri-cli --git https://github.com/tauri-apps/tauri --branch feat/open-harmony`, provides the `ohos` subcommand); desktop dev uses npm `@tauri-apps/cli`. `npm run tauri ohos ...` does not exist.
- `tauri-plugin-dialog` must stay OHOS-excluded in three synchronized places — Cargo.toml dependency section, plugin registration in `lib.rs`, and capabilities (`src-tauri/capabilities/dialog.json`, platform-scoped) — its `rfd` backend pulls GTK into OHOS builds.
- `cargo tauri ohos build` must run from the repo root (beforeBuildCommand resolves `package.json` from CLI cwd); on Windows use `npm.cmd`. **HAP packaging and signing work end-to-end** (recipe: doc §6.3 + §8; signing material in untracked `.csr/SIGNING.md` — private keys: `.csr/`, `*.p12`, `*.p7b`, `*.jks` must never be committed; AppGallery listing pending a clean aarch64 release rebuild + on-device install). **Never run standalone `hvigorw assembleHap`** — hvigor calls back into the parent CLI's WebSocket, which only exists while `cargo tauri ohos build` is running.

## CI

- `lint.yml` (PRs + main): frontend format/lint/i18n checks are advisory; **`cargo fmt` and `cargo clippy -D warnings` are hard gates**.
- `ci.yml` (main) / `pr-ci.yml` (PRs): full build matrix via reusable `build.yml` — Windows/macOS/Linux (x64 + ARM), tethering variants, Android aarch64. `release.yml` builds on GitHub release creation.
