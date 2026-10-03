# AGENTS.md

## What this repo is

Fork of [CyberTimon/RapidRAW](https://github.com/CyberTimon/RapidRAW) — a GPU-accelerated, non-destructive RAW editor — being ported to HarmonyOS/OpenHarmony. Tauri v2 app: React 19 + TypeScript frontend in `src/`, Rust backend in `src-tauri/`.

- `main` — mirrors upstream RapidRAW (Windows/macOS/Linux + Android).
- `feature/harmonyos-port` — the actual port work. **`docs/HARMONYOS_PORTING.md` (Chinese, in this branch's `docs/`) is the authoritative porting doc**: phase progress, verified command sequences, risk register. Read it before touching port code; do not reconstruct its recipes from memory.
- Port strategy: Tauri community `feat/open-harmony` branch wired in via `[patch.crates-io]` in `src-tauri/Cargo.toml`. While patches are in place, **all platforms build against the fork** (tauri 2.11.5, not upstream 2.12).

## Commands

Frontend (repo root):

- `npm start` — run the full app (`tauri dev`); `npm run dev` — frontend only (Vite, port 1420, strict)
- `npm run typecheck` — `tsc --noEmit`
- `npm run lint` / `npm run lint:fix`; `npm run format` / `npm run format:check` — Prettier (single quotes, trailing commas, 120 cols)
- `npm run tauri build` — release build; `npm run start:tethering` — dev with the `tethering` feature (camera control, macOS/Linux only, needs `libgphoto2`)

Rust — everything runs in `src-tauri/` (no workspace; `rust-toolchain.toml` pinning Rust 1.98 lives there, not at repo root):

- CI gates, exact commands: `cargo fmt -p RapidRAW -- --check` and `cargo clippy --all-targets --all-features -- -D warnings`
- Linux CI additionally installs webkit2gtk / libayatana-appindicator3 / librsvg2 / libgphoto2 system deps (`.github/workflows/lint.yml`)

i18n (13 locales, `src/i18n/locales/*.json`):

- `npm run i18n:extract` — extract/sort new keys into locale files
- `npm run i18n:check` — extraction up-to-date + plural-resolution runtime check (`src/i18n/check-runtime.mjs`)

## No test suite

No frontend tests, no Rust unit tests. Verification = lint + typecheck + clippy + fmt check + `i18n:check`, then manually exercising the affected surface in the running app. For UI smoothness changes, use the scripted replay in `bench/` (see `bench/README.md`; only scripted-vs-scripted runs on the same machine are comparable).

## Conventions & gotchas

- **All user-visible strings go through i18next** (`react-i18next` `t()`); no literal JSX text. ESLint `i18next/no-literal-string` warns, and locale JSONs must stay in sync or `i18n:check` fails.
- **First Rust build downloads ONNX Runtime**: `src-tauri/build.rs` fetches the per-platform binary from HuggingFace, sha256-verifies it into `src-tauri/resources/` (gitignored, cached afterward). Builds need network the first time. If huggingface.co is unreachable (CN networks), seed `src-tauri/resources/onnxruntime.dll` from `hf-mirror.com` (same URL path) — build.rs sha256-verifies whatever is on disk and skips the download. Runtime AI models (SAM/U-2Net/DepthAnything/CLIP/LaMa) download from the same HF repo into `app_data_dir()/models/` (Windows: `%APPDATA%\io.github.CyberTimon.RapidRAW\models`); URLs and SHA256s live in `ai_processing.rs` — the same hf-mirror seeding trick works there too.
- `src-tauri/src/lib.rs` is the monolith hub registering all ~118 Tauri IPC commands; per-feature modules sit beside it (`gpu_processing.rs`, `raw_processing.rs`, `file_management.rs`, `android_integration.rs`, …) with heavy `cfg` platform gating (Rust edition 2024).
- The web layer renders via Canvas 2D / Konva — **no WebGL/WebGPU in the frontend**. GPU work is Rust-side wgpu with two paths: direct surface rendering (desktop) and compute-only + readback + IPC bitmap (Android/Linux). OHOS reuses the readback path.
- State lives in zustand stores (`src/store/use*Store.ts`). `src/App.tsx` wires everything; views in `src/components/views/`, panels in `src/components/panel/`.
- Main window is frameless/transparent, created at runtime (`"create": false` in `tauri.conf.json`), custom titlebar in `src/window/`.
- Tauri v2 permission capabilities live in `src-tauri/capabilities/`.
- `src-tauri/gen/` (except `gen/android/`) and `src-tauri/libs/` are generated/local artifacts — gitignored, never edit.

## HarmonyOS port rules (`feature/harmonyos-port`)

Violating any of these breaks builds — they are documented in detail in `docs/HARMONYOS_PORTING.md` §6. New OHOS integration code belongs in `src-tauri/src/ohos_integration.rs` (mirrors `android_integration.rs`; Phase 2 work items are tracked in its comments and in the doc).

- **The OHOS target satisfies `target_os = "linux"`** (with `target_env = "ohos"`), so bare `target_os = "linux"` / `any(windows, linux)` gates wrongly pull GTK/webkit2gtk into OHOS builds. Use the established cfg vocabulary:
  - OHOS: `target_env = "ohos"`
  - Mobile (Android + OHOS): `any(target_os = "android", target_env = "ohos")`
  - Desktop Linux (excludes OHOS): `all(target_os = "linux", not(target_env = "ohos"))`
  - Desktop trio: `any(windows, macos, all(linux, not(ohos)))`
- **Never run bare `cargo update`.** Cargo.lock is load-bearing: the archmage trio (archmage 0.9.28 + archmage-macros 0.9.28 + magetypes 0.9.26) breaks all aarch64 builds if macros drifts, and `windows` is hand-pinned to 0.62.2 (gpu-allocator vs wgpu-hal conflict). Update with explicit package lists, then re-verify host and `aarch64-unknown-linux-ohos` checks.
- **Cross-compile env vars (`ORT_SKIP_DOWNLOAD`, `CMAKE_TOOLCHAIN_FILE`, `CMAKE_GENERATOR`, `OHOS_NDK_HOME`, plus `CC`/`CXX`/`AR` suffixed `_aarch64_unknown_linux_ohos` — cc-rs crates like `ring` need the compiler env vars; PATH wrappers alone are not enough) are shell-only** — never commit them to `.cargo/config.toml` (cargo `[env]` is global and would poison host builds, where ort-sys needs its download path). Only per-target linker/CC wrappers go in `src-tauri/.cargo/config.toml`, which is gitignored and machine-local — a fresh clone has none of it (wrapper setup: porting doc §6.1/§8).
- Focused port verification: with the §6.1 shell env set, both `cargo check` (host) and `cargo check --target aarch64-unknown-linux-ohos` in `src-tauri/` must pass.
- `OHOS_HOME`/`OHOS_NDK_HOME` must be the SDK root (`.../sdk/default/openharmony`), not the `native` dir. On Windows hosts, SDK paths must be space-free (junction alias) or cc-rs splits HMS includes and aws-lc-sys/ring fail to compile.
- OHOS builds do **not** download ONNX Runtime; each machine provides `src-tauri/libs/ohos/arm64-v8a/libonnxruntime.so` (gitignored). AI features degrade gracefully when missing.
- `src-tauri/vendor/openharmony-ability/` is a vendored pin (cargo cannot patch a git source back to itself; upstream master dropped the `webview` feature the wry fork needs) — don't update it casually. The comment above the patch table in `Cargo.toml` records how to unwind once upstream tauri ships OHOS.
- `@tauri-apps/api` is pinned to `2.11` in `package.json` to match the Rust fork — keep both in the same major.minor if the patch table changes. OHOS commands go through the cargo-installed tauri-cli fork (`cargo install tauri-cli --git https://github.com/tauri-apps/tauri --branch feat/open-harmony`, provides the `ohos` subcommand); desktop dev uses the npm `@tauri-apps/cli` (2.12.x). `npm run tauri ohos ...` does not exist.
- `tauri-plugin-dialog` must stay OHOS-excluded in three synchronized places — the Cargo.toml dependency section, plugin registration in `lib.rs`, and capabilities (already split into `src-tauri/capabilities/dialog.json` with a platform scope) — because its `rfd` backend pulls GTK into OHOS builds.
- `cargo tauri ohos build` must run from the repo root (beforeBuildCommand resolves `package.json` from CLI cwd); on Windows use `npm.cmd` (npm.ps1 is blocked). **HAP packaging works end-to-end** (unsigned HAP via `cargo tauri ohos build -d -t aarch64`; full verified recipe incl. the `ohos-devstudio\default` junction, JBR/java on PATH, and CC/CXX/AR env vars: porting doc §6.3 + §8). **Never run standalone `hvigorw assembleHap`** — hvigor calls back into the parent CLI's WebSocket (`dev-eco-studio-script`), which only exists while `cargo tauri ohos build` is running. Machine-local one-shot build script: `%TEMP%\opencode\build-ohos.ps1` (fail-fast preflight + watchdog).

## CI

- `lint.yml` (PRs + main): frontend format/lint/i18n checks are advisory (`continue-on-error`); **`cargo fmt` and `cargo clippy -D warnings` are hard gates**.
- `ci.yml` (main) / `pr-ci.yml` (PRs): full build matrix via reusable `build.yml` — Windows/macOS/Linux (x64 + ARM), tethering variants, Android aarch64. `release.yml` builds on GitHub release creation.
