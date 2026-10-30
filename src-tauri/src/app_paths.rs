//! App-data path resolution with an OpenHarmony sandbox override.
//!
//! The tauri `feat/open-harmony` fork resolves app paths with desktop XDG
//! semantics on OHOS (which satisfies `target_os = "linux"`):
//! `app_data_dir()` lands in `$HOME/.local/share/<bundle>`, which on real
//! devices is inside the FUSE user-storage mount
//! (`/storage/Users/currentUser/...`) where app processes get EPERM — it only
//! works by accident on the DevEco emulator. On hardware that breaks settings
//! load/save, background indexing (thumbnails) and LUT listing, and it leaves
//! the frontend without `editorPreviewResolution`, so the editor never even
//! creates its canvas (black screen, 2026-10-05).
//!
//! The app sandbox at `/data/storage/el2/base/...` is plain writable storage
//! on both emulators and real devices (the log dir already lives there — see
//! `setup_logging` in `lib.rs`). All RapidRAW app-path resolution goes through
//! these helpers: OHOS reads/writes the sandbox, every other platform keeps
//! the stock tauri resolver untouched.

use std::path::PathBuf;

/// Resolve a sandbox subdirectory, creating it if needed — most consumers
/// create their own subdirectories, but a few write straight into the
/// resolved dir.
#[cfg(target_env = "ohos")]
fn ohos_sandbox_dir(relative: &str) -> PathBuf {
    let dir = PathBuf::from("/data/storage/el2/base").join(relative);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Resolves to the app's persistent data directory
/// (`app_data_dir` semantics; settings, albums, presets, LUTs, AI models).
pub fn app_data_dir<R: tauri::Runtime, M: tauri::Manager<R>>(app: &M) -> tauri::Result<PathBuf> {
    #[cfg(target_env = "ohos")]
    {
        let _ = app;
        Ok(ohos_sandbox_dir("files/appdata"))
    }
    #[cfg(not(target_env = "ohos"))]
    {
        app.path().app_data_dir()
    }
}

/// Resolves to the app's configuration directory
/// (`app_config_dir` semantics; window state, crash flags).
pub fn app_config_dir<R: tauri::Runtime, M: tauri::Manager<R>>(app: &M) -> tauri::Result<PathBuf> {
    #[cfg(target_env = "ohos")]
    {
        let _ = app;
        Ok(ohos_sandbox_dir("files/config"))
    }
    #[cfg(not(target_env = "ohos"))]
    {
        app.path().app_config_dir()
    }
}

/// Resolves to the app's cache directory
/// (`app_cache_dir` semantics; thumbnails, exif cache, export bridge).
pub fn app_cache_dir<R: tauri::Runtime, M: tauri::Manager<R>>(app: &M) -> tauri::Result<PathBuf> {
    #[cfg(target_env = "ohos")]
    {
        let _ = app;
        Ok(ohos_sandbox_dir("cache"))
    }
    #[cfg(not(target_env = "ohos"))]
    {
        app.path().app_cache_dir()
    }
}

/// Resolves to the app's log directory — on OHOS this is the same sandbox
/// dir `setup_logging` writes `app.log` to.
pub fn app_log_dir<R: tauri::Runtime, M: tauri::Manager<R>>(app: &M) -> tauri::Result<PathBuf> {
    #[cfg(target_env = "ohos")]
    {
        let _ = app;
        Ok(ohos_sandbox_dir("files/logs"))
    }
    #[cfg(not(target_env = "ohos"))]
    {
        app.path().app_log_dir()
    }
}
