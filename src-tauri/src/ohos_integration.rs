//! OpenHarmony (OHOS) platform integration.
//!
//! Mirrors `android_integration.rs`: this module hosts all OpenHarmony-specific
//! glue code. It is only compiled for `*-unknown-linux-ohos` targets, which
//! report `target_os = "linux"` + `target_env = "ohos"`.
//!
//! Porting guide & progress tracker: `docs/HARMONYOS_PORTING.md`.

#[cfg(target_env = "ohos")]
pub fn initialize_ohos(window: &tauri::WebviewWindow) {
    use tauri::Manager;

    log::info!(
        "Initializing RapidRAW on OpenHarmony (window: '{}').",
        window.label()
    );

    system_bridge::set_app_handle(window.app_handle().clone());

    // Phase 2 work items (see docs/HARMONYOS_PORTING.md):
    // - File access via FileKit / photoAccessHelper: DONE (file_bridge +
    //   EntryAbility picker handlers, see 6.6/6.7).
    // - Gallery export (save_image_bytes_to_ohos_gallery): DONE (6.7).
    // - System UI adaptation: back gesture + color mode live in
    //   `system_bridge` below; safe-area/keyboard avoidance is handled on
    //   the ArkTS side (EntryAbility patch).
    // - TLS platform verifier: rustls-platform-verifier has no OHOS backend
    //   yet, so reqwest currently relies on bundled webpki roots.
}

// ---------------------------------------------------------------------------
// Window controller bridge (tao-ohos stubs workaround)
// ---------------------------------------------------------------------------
//
// tao's OHOS backend stubs `drag_window` / `set_minimized` / `set_maximized` /
// `set_fullscreen`, so the custom titlebar cannot use the regular tauri
// window APIs. The frontend instead invokes the `ohos_window_control` command
// below, which forwards through a ThreadsafeFunction to a JS callback that
// the generated EntryAbility (gen/ohos, machine-local patch — see
// docs/HARMONYOS_PORTING.md §6.4) registers at startup. The callback runs on
// the JS thread and executes the real `@ohos.window` APIs
// (minimize / maximize / restore / startMoving).

#[cfg(target_env = "ohos")]
mod window_controller {
    use std::sync::Mutex;

    use napi_ohos::bindgen_prelude::{Result, Status, Unknown};
    use napi_ohos::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};

    // CalleeHandled=false: the JS callback receives only the op string, not
    // the Node-style leading (null) error argument.
    type Controller = ThreadsafeFunction<String, Unknown<'static>, String, Status, false>;

    static CONTROLLER: Mutex<Option<Controller>> = Mutex::new(None);

    /// NAPI export (ArkTS name: `registerOhosWindowController`). Called by
    /// EntryAbility.ets during window-stage creation with a
    /// `(op: string) => void` callback.
    #[napi_derive_ohos::napi]
    pub fn register_ohos_window_controller(callback: Controller) -> Result<()> {
        CONTROLLER.lock().unwrap().replace(callback);
        log::info!("OHOS window controller registered");
        Ok(())
    }

    fn dispatch_raw(payload: String) -> std::result::Result<(), String> {
        let guard = CONTROLLER.lock().unwrap();
        match guard.as_ref() {
            Some(controller) => {
                let status = controller.call(payload, ThreadsafeFunctionCallMode::NonBlocking);
                if matches!(status, Status::Ok) {
                    Ok(())
                } else {
                    Err(format!("OHOS window controller call failed: {status:?}"))
                }
            }
            None => Err(
                "OHOS window controller not registered (EntryAbility patch missing?)".to_string(),
            ),
        }
    }

    /// Forward a window operation (plain op string) to the ArkTS side.
    pub fn dispatch(op: &str) -> std::result::Result<(), String> {
        dispatch_raw(op.to_string())
    }

    /// Forward a JSON bridge request to the ArkTS side. The payload must be a
    /// JSON object carrying at least `op` and `requestId`; the EntryAbility
    /// callback routes it to the matching handler.
    pub fn dispatch_event(payload: &str) -> std::result::Result<(), String> {
        dispatch_raw(payload.to_string())
    }
}

/// Whether this build targets OpenHarmony. The frontend uses this to route
/// titlebar window controls around the tao-ohos stubs (`platform()` from
/// tauri-plugin-os reports "linux" on OHOS, so it cannot be detected that way).
#[tauri::command]
pub fn is_ohos_build() -> bool {
    cfg!(target_env = "ohos")
}

// ---------------------------------------------------------------------------
// System UI bridge: back gesture & color mode (Phase 2 system UI adaptation)
// ---------------------------------------------------------------------------
//
// Back: the app installs its own @Entry page (gen/ohos .../pages/Index.ets,
// machine-local patch — see docs 6.5) whose page-level `onBackPress` calls
// the `notifyOhosBackPressed` NAPI export below. A page-level return of
// `true` fully consumes the gesture (ability-level `onBackPressed` can only
// background or destroy the app, which would fire even when the webview just
// closed a modal). The re-emitted `ohos-back-pressed` tauri event drives the
// same chain as the Android back button (useAndroidBackHandler.ts: close
// modals, then a synthetic Escape keydown).
//
// Color mode: EntryAbility (machine-local patch, docs 6.5) pushes the system
// color mode through `notify_ohos_color_mode` at startup and on every
// onConfigurationUpdate. The crate's Rust event loop also receives
// ConfigChanged, but tauri's OHOS runtime consumes that loop without exposing
// configuration updates to the app, hence this side channel. The frontend
// queries the initial value (`get_ohos_color_mode`) and follows changes via
// the `ohos-color-mode` event.

#[cfg(target_env = "ohos")]
mod system_bridge {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU8, Ordering};

    use napi_ohos::bindgen_prelude::Result;

    // 0 = unknown, 1 = dark, 2 = light.
    static COLOR_MODE: AtomicU8 = AtomicU8::new(0);
    static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

    pub fn set_app_handle(handle: tauri::AppHandle) {
        let _ = APP_HANDLE.set(handle);
    }

    pub fn color_mode() -> Option<bool> {
        match COLOR_MODE.load(Ordering::Relaxed) {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    fn emit(event: &str, payload: serde_json::Value) {
        if let Some(handle) = APP_HANDLE.get() {
            use tauri::Emitter;
            let _ = handle.emit(event, payload);
        }
    }

    /// NAPI export (ArkTS name: `notifyOhosBackPressed`). Called by the app's
    /// custom entry page when the user triggers the system back gesture.
    #[napi_derive_ohos::napi]
    pub fn notify_ohos_back_pressed() -> Result<()> {
        emit("ohos-back-pressed", serde_json::Value::Null);
        Ok(())
    }

    /// NAPI export (ArkTS name: `notifyOhosColorMode`). EntryAbility pushes
    /// the current system color mode here.
    #[napi_derive_ohos::napi]
    pub fn notify_ohos_color_mode(dark: bool) -> Result<()> {
        let next = if dark { 1 } else { 2 };
        let previous = COLOR_MODE.swap(next, Ordering::Relaxed);
        if previous != next {
            log::info!("OHOS system color mode changed: dark={dark}");
            emit("ohos-color-mode", serde_json::json!({ "dark": dark }));
        }
        Ok(())
    }
}

/// Latest OHOS system color mode reported by the ArkTS side, or `None` when
/// the bridge has not reported yet (always `None` on other platforms).
#[tauri::command]
pub fn get_ohos_color_mode() -> Option<bool> {
    #[cfg(target_env = "ohos")]
    {
        system_bridge::color_mode()
    }
    #[cfg(not(target_env = "ohos"))]
    {
        None
    }
}

// ---------------------------------------------------------------------------
// File bridge: FileKit pickers & save dialogs (tauri-plugin-dialog replacement)
// ---------------------------------------------------------------------------
//
// tauri-plugin-dialog is excluded from OHOS builds (its rfd backend has no
// OpenHarmony support — see Cargo.toml), so folder/file selection and save
// dialogs go through FileKit / photoAccessHelper on the ArkTS side. A request
// parks a oneshot sender in a request-id registry, forwards a JSON payload
// through the window-controller ThreadsafeFunction, and waits; the
// EntryAbility callback runs the picker and completes the channel by calling
// the matching `resolve_ohos_*` NAPI export with the request id. Requests
// are keyed by id (not a single slot) because export worker threads may run
// several saves concurrently.

#[cfg(target_env = "ohos")]
mod file_bridge {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    use napi_ohos::bindgen_prelude::Result;
    use tokio::sync::oneshot;

    // `HashMap::new` is not const, so the map is lazily created inside the
    // Option on first use.
    static REQUESTS: Mutex<Option<HashMap<u32, oneshot::Sender<String>>>> = Mutex::new(None);
    static NEXT_REQUEST_ID: AtomicU32 = AtomicU32::new(1);

    fn remove_sender(id: u32) -> Option<oneshot::Sender<String>> {
        REQUESTS
            .lock()
            .unwrap()
            .as_mut()
            .and_then(|requests| requests.remove(&id))
    }

    fn park_sender() -> (u32, oneshot::Receiver<String>) {
        let (tx, rx) = oneshot::channel();
        let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        REQUESTS
            .lock()
            .unwrap()
            .get_or_insert_with(HashMap::new)
            .insert(id, tx);
        (id, rx)
    }

    fn complete_request(id: u32, result: serde_json::Value) {
        if let Some(tx) = remove_sender(id) {
            let _ = tx.send(result.to_string());
        }
    }

    #[napi_derive_ohos::napi]
    pub fn resolve_ohos_pick_folder(request_id: u32, path: Option<String>) -> Result<()> {
        complete_request(request_id, serde_json::json!({ "path": path }));
        Ok(())
    }

    #[napi_derive_ohos::napi]
    pub fn resolve_ohos_pick_files(request_id: u32, paths: Vec<String>) -> Result<()> {
        complete_request(request_id, serde_json::json!({ "paths": paths }));
        Ok(())
    }

    #[napi_derive_ohos::napi]
    pub fn resolve_ohos_save_to_gallery(
        request_id: u32,
        ok: bool,
        error: Option<String>,
    ) -> Result<()> {
        complete_request(request_id, serde_json::json!({ "ok": ok, "error": error }));
        Ok(())
    }

    #[napi_derive_ohos::napi]
    pub fn resolve_ohos_save_file_as(
        request_id: u32,
        path: Option<String>,
        error: Option<String>,
    ) -> Result<()> {
        complete_request(
            request_id,
            serde_json::json!({ "path": path, "error": error }),
        );
        Ok(())
    }

    fn parse_result(raw: String) -> std::result::Result<serde_json::Value, String> {
        serde_json::from_str(&raw).map_err(|e| format!("Malformed OHOS bridge result ({e}): {raw}"))
    }

    fn request_payload(
        mut payload: serde_json::Value,
    ) -> std::result::Result<(u32, oneshot::Receiver<String>), String> {
        let (id, rx) = park_sender();
        payload["requestId"] = serde_json::json!(id);
        if let Err(e) = super::window_controller::dispatch_event(&payload.to_string()) {
            remove_sender(id);
            return Err(e);
        }
        Ok((id, rx))
    }

    /// Async request for tauri command contexts.
    async fn request(payload: serde_json::Value) -> std::result::Result<serde_json::Value, String> {
        let (_id, rx) = request_payload(payload)?;
        let raw = rx
            .await
            .map_err(|_| "OHOS bridge channel closed without a result".to_string())?;
        parse_result(raw)
    }

    /// Blocking request for `spawn_blocking` export worker threads.
    pub fn save_request_blocking(
        payload: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, String> {
        let (_id, rx) = request_payload(payload)?;
        let raw = rx
            .blocking_recv()
            .map_err(|_| "OHOS bridge channel closed without a result".to_string())?;
        parse_result(raw)
    }

    /// Ask the ArkTS side to open the folder picker and wait for the result.
    pub async fn pick_folder() -> std::result::Result<Option<String>, String> {
        let result = request(serde_json::json!({ "op": "pick_folder" })).await?;
        Ok(result["path"].as_str().map(str::to_string))
    }

    /// FileKit caps `DocumentSelectOptions.fileSuffixFilters` at ~100
    /// characters for the whole input; longer filter lists make the picker
    /// dismiss itself. Deduplicate the extensions, group them into a single
    /// `.a,.b` element, and drop filtering entirely when the set does not
    /// fit (callers validate picked extensions client-side, matching the
    /// Android import flow).
    fn collapse_suffix_filters(extensions: &[String]) -> Vec<String> {
        let mut unique: Vec<String> = Vec::new();
        for extension in extensions {
            let normalized = extension.to_lowercase();
            let normalized = if normalized.starts_with('.') {
                normalized
            } else {
                format!(".{normalized}")
            };
            if !unique.contains(&normalized) {
                unique.push(normalized);
            }
        }
        let grouped = unique.join(",");
        if grouped.is_empty() || grouped.len() > 96 {
            Vec::new()
        } else {
            vec![grouped]
        }
    }

    /// Ask the ArkTS side to open the multi-file picker and wait for the
    /// selected paths (empty when the user cancelled).
    pub async fn pick_files(
        extensions: &[String],
        max_select: u32,
    ) -> std::result::Result<Vec<String>, String> {
        let result = request(serde_json::json!({
            "op": "pick_files",
            "extensions": collapse_suffix_filters(extensions),
            "maxSelect": max_select,
        }))
        .await?;
        Ok(result["paths"]
            .as_array()
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(|p| p.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Ask the ArkTS side to open the system save picker; the picker creates
    /// the file and the caller writes to the returned path afterwards.
    pub async fn pick_save_file(file_name: &str) -> std::result::Result<Option<String>, String> {
        let result = request(serde_json::json!({
            "op": "save_file_as",
            "fileName": file_name,
        }))
        .await?;
        Ok(result["path"].as_str().map(str::to_string))
    }
}

/// Open the OHOS system folder picker (FileKit `DocumentViewPicker`) and
/// return the selected folder's path, or `None` when the user cancelled.
#[tauri::command]
pub async fn pick_ohos_folder() -> Result<Option<String>, String> {
    #[cfg(target_env = "ohos")]
    {
        file_bridge::pick_folder().await
    }
    #[cfg(not(target_env = "ohos"))]
    {
        Err("pick_ohos_folder is only available on OpenHarmony builds".to_string())
    }
}

/// Open the OHOS system file picker (FileKit `DocumentViewPicker`, FILE
/// mode) and return the selected files' paths. Extension filtering happens
/// picker-side; an empty result means the user cancelled.
#[tauri::command]
pub async fn pick_ohos_files(
    supported_extensions: Vec<String>,
    max_select: u32,
) -> Result<Vec<String>, String> {
    #[cfg(target_env = "ohos")]
    {
        file_bridge::pick_files(&supported_extensions, max_select).await
    }
    #[cfg(not(target_env = "ohos"))]
    {
        let _ = (supported_extensions, max_select);
        Err("pick_ohos_files is only available on OpenHarmony builds".to_string())
    }
}

/// Open the OHOS system save picker (FileKit `DocumentSaveOptions`); the
/// picker creates the file and returns its path, or `None` when cancelled.
#[tauri::command]
pub async fn pick_ohos_save_file(file_name: String) -> Result<Option<String>, String> {
    #[cfg(target_env = "ohos")]
    {
        file_bridge::pick_save_file(&file_name).await
    }
    #[cfg(not(target_env = "ohos"))]
    {
        let _ = file_name;
        Err("pick_ohos_save_file is only available on OpenHarmony builds".to_string())
    }
}

// ---------------------------------------------------------------------------
// Export write bridge: gallery / save-picker (Android MediaStore equivalent)
// ---------------------------------------------------------------------------
//
// `photoAccessHelper` and the FileKit save picker are ArkTS-only APIs, and
// exported images can be tens of megabytes — too large to push through the
// window-controller ThreadsafeFunction. Instead the bytes are parked in a
// temp file inside the app cache dir (readable from ArkTS — same process)
// and only the path crosses the bridge.

#[cfg(target_env = "ohos")]
static OHOS_EXPORT_TEMP_COUNTER: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Write `bytes` to a uniquely named temp file for the ArkTS export bridge.
#[cfg(target_env = "ohos")]
fn ohos_export_temp_file(
    app_handle: &tauri::AppHandle,
    file_name: &str,
    bytes: &[u8],
) -> std::result::Result<std::path::PathBuf, String> {
    use tauri::Manager;

    let base = app_handle
        .path()
        .app_cache_dir()
        .map_err(|e| format!("Failed to resolve OHOS app cache dir: {e}"))?;
    let dir = base.join("export_bridge");
    std::fs::create_dir_all(&dir).map_err(|e| {
        format!(
            "Failed to create OHOS export bridge dir '{}': {e}",
            dir.display()
        )
    })?;
    let unique = OHOS_EXPORT_TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = dir.join(format!("{unique}_{file_name}"));
    std::fs::write(&path, bytes)
        .map_err(|e| format!("Failed to write export temp file '{}': {e}", path.display()))?;
    Ok(path)
}

/// Save an exported image into the OHOS media library (gallery) — the
/// equivalent of `save_image_bytes_to_android_gallery`. Called from export
/// worker threads (`spawn_blocking`), hence the blocking bridge request.
#[cfg(target_env = "ohos")]
pub fn save_image_bytes_to_ohos_gallery(
    app_handle: &tauri::AppHandle,
    file_name: &str,
    mime_type: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let temp_path = ohos_export_temp_file(app_handle, file_name, bytes)?;
    let result = file_bridge::save_request_blocking(serde_json::json!({
        "op": "save_to_gallery",
        "tempPath": temp_path.to_string_lossy(),
        "fileName": file_name,
        "mimeType": mime_type,
    }));
    // Best-effort cleanup in case the ArkTS side did not consume the file.
    let _ = std::fs::remove_file(&temp_path);
    let result = result?;
    if result["ok"].as_bool().unwrap_or(false) {
        Ok(())
    } else {
        Err(result["error"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| format!("OHOS gallery save failed for '{file_name}'")))
    }
}

/// Save exported bytes (e.g. a `.cube` LUT) to a user-chosen location via
/// the FileKit save picker — the equivalent of
/// `save_file_bytes_to_android_downloads`.
#[cfg(target_env = "ohos")]
pub fn save_file_bytes_to_ohos_picker(
    app_handle: &tauri::AppHandle,
    file_name: &str,
    mime_type: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let temp_path = ohos_export_temp_file(app_handle, file_name, bytes)?;
    let result = file_bridge::save_request_blocking(serde_json::json!({
        "op": "save_file_as",
        "tempPath": temp_path.to_string_lossy(),
        "fileName": file_name,
        "mimeType": mime_type,
    }));
    let _ = std::fs::remove_file(&temp_path);
    match result?["path"].as_str() {
        Some(path) if !path.is_empty() => Ok(()),
        // A missing path means the user cancelled the save dialog.
        _ => Err(format!("OHOS save picker cancelled for '{file_name}'")),
    }
}

/// Drive an OHOS main-window operation through the ArkTS bridge.
///
/// Supported operations: `minimize`, `maximize`, `restore`, `start_drag`.
#[tauri::command]
pub fn ohos_window_control(operation: String) -> Result<(), String> {
    #[cfg(target_env = "ohos")]
    {
        window_controller::dispatch(&operation)
    }
    #[cfg(not(target_env = "ohos"))]
    {
        Err(format!(
            "ohos_window_control('{operation}') is only available on OpenHarmony"
        ))
    }
}
