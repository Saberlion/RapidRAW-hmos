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

/// Terminate the app. A raw process exit registers as an abnormal ability
/// death on OHOS, so termination routes through the ArkTS side
/// (`terminateSelf`, wired as the `exit_app` window op in
/// EntryAbility.template.ets); desktop exits through the Tauri handle.
#[tauri::command]
pub fn exit_app(app_handle: tauri::AppHandle) {
    #[cfg(target_env = "ohos")]
    {
        let _ = &app_handle;
        if let Err(e) = window_controller::dispatch_event("exit_app") {
            log::error!("exit_app bridge dispatch failed: {e}");
        }
    }
    #[cfg(not(target_env = "ohos"))]
    {
        app_handle.exit(0);
    }
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

    /// Emit a tauri event to the webview (shared with sibling bridge modules).
    pub(super) fn emit(event: &str, payload: serde_json::Value) {
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

    /// Complete a parked request. Visible to sibling bridge modules (e.g.
    /// `privacy`) that share this request registry.
    pub(crate) fn complete_request(id: u32, result: serde_json::Value) {
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

    #[napi_derive_ohos::napi]
    pub fn resolve_ohos_revoke_grants(request_id: u32, payload: String) -> Result<()> {
        let value = serde_json::from_str(&payload).unwrap_or_else(|e| {
            serde_json::json!({
                "ok": false,
                "error": format!("Malformed OHOS grant payload ({e}): {payload}"),
            })
        });
        complete_request(request_id, value);
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
    pub(crate) async fn request(
        payload: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, String> {
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

    /// Withdraw recorded folder grants: the ArkTS side deactivates the persisted
    /// `fileShare` permission for each identifier and forgets the record, so a
    /// later launch cannot re-activate it. `identifiers: None` withdraws every
    /// recorded grant; identifiers are picker URIs or resolved paths.
    pub async fn revoke_grants(
        identifiers: Option<Vec<String>>,
    ) -> std::result::Result<serde_json::Value, String> {
        request(serde_json::json!({
            "op": "revoke_folder_grants",
            "uris": identifiers,
        }))
        .await
    }
}

/// Open the OHOS system folder picker (FileKit `DocumentViewPicker`) and
/// return the selected folder's path, or `None` when the user cancelled.
///
/// Gated on the AppGallery privacy consent: the picker requests a file-access
/// grant, and the platform refuses grants while the hosted privacy statement is
/// unsigned (see `privacy::ensure_consent`).
#[tauri::command]
pub async fn pick_ohos_folder() -> Result<Option<String>, String> {
    #[cfg(target_env = "ohos")]
    {
        privacy::ensure_consent().await?;
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
///
/// Gated on the AppGallery privacy consent, like `pick_ohos_folder`.
#[tauri::command]
pub async fn pick_ohos_files(
    supported_extensions: Vec<String>,
    max_select: u32,
) -> Result<Vec<String>, String> {
    #[cfg(target_env = "ohos")]
    {
        privacy::ensure_consent().await?;
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
///
/// Gated on the AppGallery privacy consent, like `pick_ohos_folder`.
#[tauri::command]
pub async fn pick_ohos_save_file(file_name: String) -> Result<Option<String>, String> {
    #[cfg(target_env = "ohos")]
    {
        privacy::ensure_consent().await?;
        file_bridge::pick_save_file(&file_name).await
    }
    #[cfg(not(target_env = "ohos"))]
    {
        let _ = file_name;
        Err("pick_ohos_save_file is only available on OpenHarmony builds".to_string())
    }
}

/// Withdraw OpenHarmony folder grants.
///
/// `fileShare` grants persisted by the picker (`persistFolderGrant`) are
/// re-activated on every launch, so removing a library root — or revoking the
/// privacy consent, whose confirmation promises that file access has to be
/// re-authorized — only takes effect once the grant is both deactivated and
/// dropped from the bridge's record. `identifiers: None` withdraws every
/// recorded grant (privacy revoke); a list withdraws just those entries, matched
/// by picker URI or resolved path (removing a root folder).
///
/// Returns the ArkTS payload (`ok`, `revoked`, `errors`); a non-OHOS build has
/// no such grants and reports that instead of pretending to have revoked them.
#[tauri::command]
pub async fn revoke_ohos_folder_grants(
    identifiers: Option<Vec<String>>,
) -> Result<serde_json::Value, String> {
    #[cfg(target_env = "ohos")]
    {
        file_bridge::revoke_grants(identifiers).await
    }
    #[cfg(not(target_env = "ohos"))]
    {
        let _ = identifiers;
        Err("revoke_ohos_folder_grants is only available on OpenHarmony builds".to_string())
    }
}

// ---------------------------------------------------------------------------
// External link bridge: open http(s)/mailto/tel URLs in the system browser
// ---------------------------------------------------------------------------//
// tauri-plugin-shell cannot do this on OHOS. Its `open` command only takes the
// native path when `cfg(mobile)` holds (android | ios), so an OHOS build
// compiles the desktop branch, which shells out to `xdg-open` through the
// `open` crate. HarmonyOS has no `xdg-open`, so every external link click ended
// as an unhandled rejection:
//   `Scoped shell IO error: No such file or directory (os error 2)`
// — and because the plugin's injected init script also `preventDefault()`s the
// click (so the webview never navigates either), the link did nothing at all
// (docs 6.19). The URL therefore goes through the same request bridge as the
// pickers: the ArkTS side opens it with an `ohos.want.action.viewData` want.

/// Schemes accepted for external links. Mirrors what tauri-plugin-shell's
/// injected init script accepts (`src/init-iife.js`), so routing links through
/// this bridge grants no reach the previous behaviour did not already have.
const EXTERNAL_URL_SCHEMES: [&str; 4] = ["http://", "https://", "mailto:", "tel:"];

fn is_openable_url(url: &str) -> bool {
    EXTERNAL_URL_SCHEMES
        .iter()
        .any(|scheme| url.starts_with(scheme))
}

#[cfg(target_env = "ohos")]
mod external_open {
    use napi_ohos::bindgen_prelude::Result;

    /// Ask the ArkTS side to launch the system browser for `url`.
    pub async fn open(url: &str) -> std::result::Result<(), String> {
        let result = super::file_bridge::request(serde_json::json!({
            "op": "open_url",
            "url": url,
        }))
        .await?;

        match result["ok"].as_bool() {
            Some(true) => Ok(()),
            _ => Err(result["error"]
                .as_str()
                .unwrap_or("opening the external URL failed")
                .to_string()),
        }
    }

    #[napi_derive_ohos::napi]
    pub fn resolve_ohos_open_url(request_id: u32, ok: bool, error: Option<String>) -> Result<()> {
        super::file_bridge::complete_request(
            request_id,
            serde_json::json!({ "ok": ok, "error": error }),
        );
        Ok(())
    }
}

/// Open an external URL in the system browser.
///
/// OHOS goes through the ArkTS bridge (see the note above); every other
/// platform keeps using tauri-plugin-shell, which is correct there.
///
/// `Shell::open` is marked deprecated in favour of `tauri-plugin-opener`, which
/// this project does not depend on — the frontend has always opened links
/// through the shell plugin, so desktop behaviour is deliberately unchanged
/// here.
#[tauri::command]
#[allow(deprecated)]
pub async fn open_external_url(app_handle: tauri::AppHandle, url: String) -> Result<(), String> {
    if !is_openable_url(&url) {
        return Err(format!("refusing to open unsupported URL: {url}"));
    }

    #[cfg(target_env = "ohos")]
    {
        let _ = app_handle;
        external_open::open(&url).await
    }
    #[cfg(not(target_env = "ohos"))]
    {
        use tauri_plugin_shell::ShellExt;
        app_handle
            .shell()
            .open(url, None)
            .map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Privacy bridge: AppGallery standardized privacy-statement hosting
// ---------------------------------------------------------------------------
//
// When an app uses AppGallery's standardized privacy-statement hosting service,
// the STORE owns first-launch consent: the platform shows its own privacy
// dialog and the app must NOT draw a second one (AppGallery rejects builds that
// do). `App.tsx` already skips the in-app gate on OHOS for that reason (docs
// 6.17); this bridge adds the active half of the integration — the app can
//   - show the store-issued privacy statement / user agreement links,
//   - read the current consent state, including store-side changes pushed as
//     the `usual.event.PRIVACY_STATE_CHANGED` common event,
//   - revoke a recorded consent, and
//   - proactively re-request consent before a permission-gated operation
//     (the FileKit pickers above).
//
// The service is unavailable on emulators and in builds without the hosted
// statement (AppGallery error 1006700003). Every entry point FAILS OPEN: the
// app keeps working and the platform still refuses the underlying permission
// grant itself, which is where the guarantee actually lives.

#[cfg(target_env = "ohos")]
pub mod privacy {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use napi_ohos::bindgen_prelude::Result;

    /// Last consent verdict derived from a bridge response or a
    /// `PRIVACY_STATE_CHANGED` push. `None` = not (yet) reported, or reported
    /// as unknown (hosted statement missing / service unavailable).
    static CONSENT_GRANTED: Mutex<Option<bool>> = Mutex::new(None);

    /// Latched once the service reports that hosting is not in use (or the
    /// device cannot use it), so permission operations skip the gate for the
    /// rest of the process instead of probing + failing on every picker.
    static SERVICE_UNAVAILABLE: AtomicBool = AtomicBool::new(false);

    /// AppGallery error code for "this app does not use privacy statement
    /// hosting" — mirrors the frontend's `PRIVACY_NOT_INTEGRATED`.
    const PRIVACY_NOT_INTEGRATED: i64 = 1006700003;

    fn set_consent(granted: Option<bool>) {
        *CONSENT_GRANTED.lock().unwrap() = granted;
    }

    /// Last known consent verdict, if any.
    pub fn consent_granted() -> Option<bool> {
        *CONSENT_GRANTED.lock().unwrap()
    }

    /// Derive a verdict from an ArkTS privacy payload. `isAgreed` wins when the
    /// ArkTS side provides it (`ConsentResult`); otherwise the per-document
    /// `results[]` are collapsed: all signed (result == 1) = granted, an empty
    /// list = unknown (no hosted statement in play).
    fn payload_granted(payload: &serde_json::Value) -> Option<bool> {
        if let Some(agreed) = payload.get("isAgreed").and_then(serde_json::Value::as_bool) {
            return Some(agreed);
        }
        let results = payload.get("results")?.as_array()?;
        if results.is_empty() {
            return None;
        }
        Some(
            results
                .iter()
                .all(|r| r.get("result").and_then(serde_json::Value::as_i64) == Some(1)),
        )
    }

    /// NAPI export (ArkTS name: `resolveOhosPrivacy`). Completes a parked
    /// privacy bridge request with the ArkTS-side JSON payload.
    #[napi_derive_ohos::napi]
    pub fn resolve_ohos_privacy(request_id: u32, payload: String) -> Result<()> {
        let value = serde_json::from_str(&payload).unwrap_or_else(|e| {
            serde_json::json!({
                "ok": false,
                "error": format!("Malformed OHOS privacy payload ({e}): {payload}"),
            })
        });
        super::file_bridge::complete_request(request_id, value);
        Ok(())
    }

    /// NAPI export (ArkTS name: `notifyOhosPrivacyChanged`). The ArkTS side
    /// pushes the `usual.event.PRIVACY_STATE_CHANGED` payload here; the webview
    /// receives it as the `ohos-privacy-state` tauri event.
    #[napi_derive_ohos::napi]
    pub fn notify_ohos_privacy_changed(payload: String) -> Result<()> {
        let parsed: serde_json::Value =
            serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null);
        let granted = parsed
            .get("granted")
            .and_then(serde_json::Value::as_bool)
            .or_else(|| {
                parsed
                    .get("resultType")
                    .and_then(serde_json::Value::as_i64)
                    .map(|value| value == 1)
            });
        if let Some(granted) = granted {
            log::info!("OHOS privacy consent changed: granted={granted}");
            set_consent(Some(granted));
        }
        super::system_bridge::emit("ohos-privacy-state", parsed);
        Ok(())
    }

    /// Send one privacy op over the shared request registry.
    async fn call(op: &str) -> std::result::Result<serde_json::Value, String> {
        super::file_bridge::request(serde_json::json!({ "op": op })).await
    }

    fn is_ok(payload: &serde_json::Value) -> bool {
        payload.get("ok").and_then(serde_json::Value::as_bool) == Some(true)
    }

    /// Store-issued privacy links + current consent state.
    pub async fn state() -> std::result::Result<serde_json::Value, String> {
        let payload = call("privacy_state").await?;
        if is_ok(&payload) {
            set_consent(payload_granted(&payload));
        }
        Ok(payload)
    }

    /// Raise the platform's standardized privacy dialog, returning its result.
    pub async fn request_consent() -> std::result::Result<serde_json::Value, String> {
        let payload = call("privacy_request_consent").await?;
        if is_ok(&payload) {
            set_consent(payload_granted(&payload));
        }
        Ok(payload)
    }

    /// Revoke the recorded consent (the next launch shows the dialog again).
    ///
    /// Only a *verified* revoke clears the cached verdict — the payload now
    /// comes back from the ArkTS side after re-reading `getAppPrivacyResult`,
    /// because `disableService()` reports nothing (docs 6.20). A payload whose
    /// code is "hosting not integrated" latches the same flag the probe uses, so
    /// pickers stop gating on a service this build cannot use.
    pub async fn revoke() -> std::result::Result<serde_json::Value, String> {
        let payload = call("privacy_revoke").await?;
        if is_ok(&payload) {
            set_consent(Some(false));
        } else {
            log::warn!("OHOS privacy revoke not confirmed: {payload}");
            if payload.get("code").and_then(serde_json::Value::as_i64)
                == Some(PRIVACY_NOT_INTEGRATED)
            {
                SERVICE_UNAVAILABLE.store(true, Ordering::Relaxed);
            }
        }
        Ok(payload)
    }

    /// Consent gate for permission-requiring operations (the FileKit pickers).
    ///
    /// Fails OPEN by design: when the hosted statement is missing or the service
    /// errors out, the caller proceeds exactly as before — the platform denies
    /// the permission grant itself in that case, so an emulator or an unlisted
    /// debug build must not become unusable. Only an explicit rejection of the
    /// dialog raised here blocks the operation.
    pub async fn ensure_consent() -> std::result::Result<(), String> {
        if matches!(consent_granted(), Some(true)) {
            return Ok(());
        }
        if SERVICE_UNAVAILABLE.load(Ordering::Relaxed) {
            return Ok(());
        }
        match state().await {
            Ok(payload) => {
                if !is_ok(&payload) {
                    log::warn!("OHOS privacy hosting is not in use; skipping the consent gate");
                    SERVICE_UNAVAILABLE.store(true, Ordering::Relaxed);
                    return Ok(());
                }
                if payload_granted(&payload) == Some(true) {
                    return Ok(());
                }
            }
            Err(e) => {
                log::warn!("OHOS privacy state unavailable ({e}); skipping the consent gate");
                return Ok(());
            }
        }
        match request_consent().await {
            Ok(payload) => {
                if !is_ok(&payload) {
                    log::warn!(
                        "OHOS privacy consent dialog unavailable; skipping the consent gate"
                    );
                    SERVICE_UNAVAILABLE.store(true, Ordering::Relaxed);
                    return Ok(());
                }
                match payload_granted(&payload) {
                    Some(true) => Ok(()),
                    // A user rejection stays retryable: the next attempt asks again.
                    Some(false) => Err("The privacy statement has not been accepted; AppGallery \
                         requires consent before a file access grant can be requested"
                        .to_string()),
                    // No document reported back: hosting is effectively not in
                    // play, so stop asking on every picker.
                    None => {
                        log::warn!(
                            "OHOS privacy consent dialog reported no documents; skipping the consent gate"
                        );
                        SERVICE_UNAVAILABLE.store(true, Ordering::Relaxed);
                        Ok(())
                    }
                }
            }
            Err(e) => {
                log::warn!("OHOS privacy consent request failed ({e}); skipping the consent gate");
                Ok(())
            }
        }
    }
}

/// Query the AppGallery-issued privacy links and the current consent state.
/// Returns the ArkTS payload as-is (`ok: false` + `error`/`code` when the
/// hosted statement is not in play); only bridge/transport failures are `Err`.
#[tauri::command]
pub async fn get_ohos_privacy_state() -> Result<serde_json::Value, String> {
    #[cfg(target_env = "ohos")]
    {
        privacy::state().await
    }
    #[cfg(not(target_env = "ohos"))]
    {
        Err("get_ohos_privacy_state is only available on OpenHarmony builds".to_string())
    }
}

/// Raise AppGallery's standardized privacy dialog and return its result.
#[tauri::command]
pub async fn request_ohos_privacy_consent() -> Result<serde_json::Value, String> {
    #[cfg(target_env = "ohos")]
    {
        privacy::request_consent().await
    }
    #[cfg(not(target_env = "ohos"))]
    {
        Err("request_ohos_privacy_consent is only available on OpenHarmony builds".to_string())
    }
}

/// Revoke the recorded AppGallery privacy consent.
#[tauri::command]
pub async fn revoke_ohos_privacy_consent() -> Result<serde_json::Value, String> {
    #[cfg(target_env = "ohos")]
    {
        privacy::revoke().await
    }
    #[cfg(not(target_env = "ohos"))]
    {
        Err("revoke_ohos_privacy_consent is only available on OpenHarmony builds".to_string())
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
    let base = crate::app_paths::app_cache_dir(app_handle)
        .map_err(|e| format!("Failed to resolve OHOS app cache dir: {e}"))?;
    // This must stay distinct from the ArkTS staging dir
    // (`context.cacheDir/export_bridge`): the bridge stages tempPath into that
    // dir with `fs.copyFileSync`, and OHOS fs fails with 13900002 when source
    // and destination are the same file. On real devices the resolver's cache
    // dir IS the ability's cacheDir, so a shared "export_bridge" name made
    // every staged path identical to its source.
    let dir = base.join("export_temp");
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

// ---------------------------------------------------------------------------
// MindSpore Lite NNRT (NPU) backend for AI denoise (Phase 4)
// ---------------------------------------------------------------------------
//
// Ports the device-proven FFI + tile loop from the nindfull POC
// (`mslite-poc/nindfull/src/main.rs` ohos_backend mod). The safe wrapper
// `MsModel` holds the raw model/context pointers and the cached input/output
// tensor-handle arrays; `DenoiseSession::Nnrt(Mutex<MsModel>)` in
// ai_processing.rs enforces sequential predict calls.
//
// Device-proven MS Lite rules: ModelPredict inputs BY VALUE (the
// TensorHandleArray struct is passed by value); output tensor handles are
// reused across Predict calls and NEVER destroyed (destroying them
// segfaults); teardown = ModelDestroy + ContextDestroy only — and we omit
// even that (leak-until-exit; process exit reclaims). See POC comments and
// `docs/HARMONYOS_PORTING.md`.

#[cfg(target_env = "ohos")]
pub mod ms_lite {
    use std::cell::UnsafeCell;
    use std::ffi::{CString, c_char, c_void};
    use std::path::Path;

    use anyhow::Result;

    const OH_AI_MODELTYPE_MINDIR: i32 = 0;
    const OH_AI_DEVICETYPE_CPU: i32 = 0;
    const OH_AI_NNRTDEVICE_ACCELERATOR: i32 = 3;
    const OH_AI_PERFORMANCE_HIGH: i32 = 3;
    const OH_AI_PRIORITY_HIGH: i32 = 3;
    const OH_AI_STATUS_SUCCESS: i32 = 0;

    /// C-ABI tensor handle array — passed BY VALUE to `OH_AI_ModelPredict`
    /// (passing a pointer empirically segfaults; the Kit copies the struct).
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct TensorHandleArray {
        pub handle_num: usize,
        pub handle_list: *mut *mut c_void,
    }

    #[link(name = "mindspore_lite_ndk")]
    unsafe extern "C" {
        fn OH_AI_ContextCreate() -> *mut c_void;
        fn OH_AI_ContextSetThreadNum(context: *mut c_void, thread_num: i32);
        fn OH_AI_ContextAddDeviceInfo(context: *mut c_void, device_info: *mut c_void);
        fn OH_AI_DeviceInfoCreate(device_type: i32) -> *mut c_void;
        fn OH_AI_DeviceInfoSetPerformanceMode(device_info: *mut c_void, mode: i32);
        fn OH_AI_DeviceInfoSetPriority(device_info: *mut c_void, priority: i32);
        fn OH_AI_CreateNNRTDeviceInfoByType(device_type: i32) -> *mut c_void;
        fn OH_AI_ModelCreate() -> *mut c_void;
        fn OH_AI_ModelBuildFromFile(
            model: *mut c_void,
            model_path: *const c_char,
            model_type: i32,
            model_context: *mut c_void,
        ) -> i32;
        fn OH_AI_ModelPredict(
            model: *mut c_void,
            inputs: TensorHandleArray,
            outputs: *mut TensorHandleArray,
            before: *const c_void,
            after: *const c_void,
        ) -> i32;
        fn OH_AI_ModelGetInputs(model: *const c_void) -> TensorHandleArray;
        fn OH_AI_TensorGetShape(tensor: *const c_void, shape_num: *mut usize) -> *const i64;
        fn OH_AI_TensorGetData(tensor: *const c_void) -> *const c_void;
        fn OH_AI_TensorGetMutableData(tensor: *mut c_void) -> *mut c_void;
    }

    /// Safe wrapper around a built MindSpore Lite model. Holds the raw model
    /// pointer, the cached input tensor-handle array (reused across Predict),
    /// and the output tensor-handle array (reused across Predict, never
    /// destroyed).
    ///
    /// The context pointer is intentionally NOT stored: after
    /// `ModelBuildFromFile` the model holds an internal reference to it, and
    /// the leak-until-exit design (no Drop) means the context allocation
    /// persists for the process lifetime regardless. Storing it would only be
    /// needed for a future safe Drop (ModelDestroy + ContextDestroy).
    pub struct MsModel {
        model: *mut c_void,
        inputs: TensorHandleArray,
        // Reused across Predict calls — the Kit fills this in and reuses the
        // handles. Accessed via UnsafeCell because predict takes &self (the
        // struct lives behind a Mutex; only one predict runs at a time).
        outputs: UnsafeCell<TensorHandleArray>,
        cs: usize,
    }

    // SAFETY: MsModel is always accessed behind `Mutex<MsModel>` (via
    // `DenoiseSession::Nnrt`). Predict calls are strictly sequential — the
    // app serializes denoise jobs. The MindSpore Lite C API is not
    // thread-safe; the Mutex enforces mutual exclusion. The UnsafeCell
    // <outputs> is only mutated inside `predict`, which requires holding the
    // Mutex guard, so there is no concurrent &mut access.
    unsafe impl Send for MsModel {}
    unsafe impl Sync for MsModel {}

    impl MsModel {
        /// Build a model from a `.ms` file with NNRT (NPU accelerator, HIGH
        /// perf/priority) + CPU fallback. Validates the input shape is
        /// `[1, 504, 504, 3]` (NHWC).
        pub fn new(model_path: &Path) -> Result<Self> {
            let context = unsafe { OH_AI_ContextCreate() };
            if context.is_null() {
                anyhow::bail!("OH_AI_ContextCreate returned null");
            }
            unsafe { OH_AI_ContextSetThreadNum(context, 4) };

            // NNRT (NPU accelerator) device + CPU device as fallback.
            unsafe {
                let nnrt_dev = OH_AI_CreateNNRTDeviceInfoByType(OH_AI_NNRTDEVICE_ACCELERATOR);
                if nnrt_dev.is_null() {
                    anyhow::bail!("OH_AI_CreateNNRTDeviceInfoByType returned null");
                }
                OH_AI_DeviceInfoSetPerformanceMode(nnrt_dev, OH_AI_PERFORMANCE_HIGH);
                OH_AI_DeviceInfoSetPriority(nnrt_dev, OH_AI_PRIORITY_HIGH);
                OH_AI_ContextAddDeviceInfo(context, nnrt_dev);
                let cpu_dev = OH_AI_DeviceInfoCreate(OH_AI_DEVICETYPE_CPU);
                if !cpu_dev.is_null() {
                    OH_AI_ContextAddDeviceInfo(context, cpu_dev);
                }
            }

            let model = unsafe { OH_AI_ModelCreate() };
            if model.is_null() {
                anyhow::bail!("OH_AI_ModelCreate returned null");
            }

            let c_path = CString::new(model_path.to_string_lossy().into_owned())
                .map_err(|e| anyhow::anyhow!("CString conversion failed: {e}"))?;

            // SAFETY: c_path is a valid null-terminated C string; context was
            // created above and is consumed by the model build.
            let status = unsafe {
                OH_AI_ModelBuildFromFile(model, c_path.as_ptr(), OH_AI_MODELTYPE_MINDIR, context)
            };
            if status != OH_AI_STATUS_SUCCESS {
                anyhow::bail!("ModelBuildFromFile status={status}");
            }

            let inputs = unsafe { OH_AI_ModelGetInputs(model) };
            if inputs.handle_num == 0 || inputs.handle_list.is_null() {
                anyhow::bail!("ModelGetInputs returned no input tensors");
            }

            // Validate input shape == [1, 504, 504, 3] (NHWC).
            let cs = 504usize;
            // SAFETY: in_tensor is the first handle from ModelGetInputs; the
            // model owns the tensor memory for its lifetime.
            unsafe {
                let in_tensor = *inputs.handle_list;
                let mut sn: usize = 0;
                let shape = OH_AI_TensorGetShape(in_tensor, &mut sn);
                let dims: Vec<i64> = if shape.is_null() || sn == 0 {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(shape, sn).to_vec()
                };
                if dims != [1i64, 504, 504, 3] {
                    anyhow::bail!("unexpected input shape {dims:?} (want NHWC [1,504,504,3])");
                }
            }

            log::info!("MindSpore Lite model loaded (NNRT + CPU fallback, cs={cs})");

            Ok(Self {
                model,
                inputs,
                outputs: UnsafeCell::new(TensorHandleArray {
                    handle_num: 0,
                    handle_list: std::ptr::null_mut(),
                }),
                cs,
            })
        }

        /// Run a single tile prediction. `input_nhwc` is flat NHWC
        /// `[cs*cs*3]`; returns flat NCHW `[3*cs*cs]`.
        ///
        /// The output `TensorHandleArray` is reused across calls (never
        /// destroyed — POC-proven that destroying Predict outputs segfaults).
        pub fn predict(&self, input_nhwc: &[f32]) -> Result<Vec<f32>> {
            let expected_len = 3 * self.cs * self.cs;
            if input_nhwc.len() != expected_len {
                anyhow::bail!(
                    "input length {} != expected {}",
                    input_nhwc.len(),
                    expected_len
                );
            }

            // SAFETY: outputs is only accessed here, behind the Mutex.
            let outputs = unsafe { &mut *self.outputs.get() };

            // Fill input tensor data.
            // SAFETY: in_tensor was validated in new(); GetMutableData returns
            // a mutable pointer to the tensor's data buffer (expected_len f32).
            let data_ptr = unsafe {
                let in_tensor = *self.inputs.handle_list;
                OH_AI_TensorGetMutableData(in_tensor)
            } as *mut f32;
            if data_ptr.is_null() {
                anyhow::bail!("GetMutableData returned null");
            }
            unsafe {
                std::ptr::copy_nonoverlapping(input_nhwc.as_ptr(), data_ptr, expected_len);
            }

            // SAFETY: model was built in new(); inputs passed BY VALUE (the
            // Kit copies the TensorHandleArray struct — passing a pointer
            // segfaults, POC-proven); outputs reused across calls.
            let status = unsafe {
                OH_AI_ModelPredict(
                    self.model,
                    self.inputs,
                    outputs,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            };
            if status != OH_AI_STATUS_SUCCESS {
                anyhow::bail!("ModelPredict status={status}");
            }

            if outputs.handle_num == 0 || outputs.handle_list.is_null() {
                anyhow::bail!("no output tensors after Predict");
            }

            // Read NCHW output. NEVER destroy the output handle.
            // SAFETY: outputs was filled by Predict; handle_list[0] is the
            // output tensor. GetData returns a const pointer to output data.
            let out_data = unsafe {
                let out_tensor = *outputs.handle_list;
                OH_AI_TensorGetData(out_tensor)
            } as *const f32;
            if out_data.is_null() {
                anyhow::bail!("output data null");
            }

            let mut result = vec![0f32; expected_len];
            unsafe {
                std::ptr::copy_nonoverlapping(out_data, result.as_mut_ptr(), expected_len);
            }
            Ok(result)
        }

        pub fn cs(&self) -> usize {
            self.cs
        }
    }

    // No Drop impl — leak-until-exit design. The POC proved that destroying
    // Predict outputs segfaults; safe teardown would be ModelDestroy +
    // ContextDestroy only (a future TODO if per-toggle leaks become
    // problematic). Process exit reclaims all allocations.
}

// ---------------------------------------------------------------------------
// CANN (.om / HIAI_MR) NPU backend for the NIND denoise model
// (mask models run on ORT; see docs/AI_MODELS_V1.6.7.md)
// ---------------------------------------------------------------------------
//
// Runs omg-built `.om` models through the HiAI Foundation Model Runtime
// (HIAI_MR_*) C API. Unlike ms_lite (an NDK library linked at build time),
// this runtime lives in on-device vendor libraries that are not part of the
// NDK, so the three facades are dlopen'd lazily at runtime, in dependency
// order so each lib's DT_NEEDED sons resolve against the already-loaded
// handles:
//   /vendor/lib64/passthrough/indirect/libai_fmk_tensor.so
//   /vendor/lib64/passthrough/indirect/libai_fmk_model_runtime_core.so
//   /vendor/lib64/passthrough/indirect/libai_fmk_hcl_model_runtime.so
// (each is also tried by bare soname in case the app's linker namespace
// already resolves it). Any load failure degrades to the caller's ORT
// fallback — dlopen is only attempted when a sideloaded `.om` exists.
//
// The full lifecycle is device-proven by the campaign harness (reference C
// implementation: hiai_mr_test7.c; recipes and measurements:
// docs/MINDSPORE_LITE_NPU_EVAL.md §8.4). Device-proven rules baked in:
// - Buffers MUST be created through the HCL facade's
//   `HIAI_MR_NDTensorBuffer_CreateFromNDTensorDesc` — ION/DMA-backed, with
//   every buffer's fd validated >= 0. Plain tensor.so host-memory buffers
//   SIGSEGV inside Run (RunV3 requires ION backing).
// - `HIAI_HCL_ModelInitOptions_SetPerfMode`'s return value is garbage —
//   the applied mode is verified through the Get accessor instead. Mode 4
//   is the measured optimum for small-op-dense CNN graphs (NIND 1.8×,
//   vs default; transformers/FFC graphs keep the default 2).
// - Destroy entry points take the ADDRESS of the handle pointer.
// - Teardown order: buffers → ModelManager_Deinit → ModelManager_Destroy
//   → ModelInitOptions_Destroy → BuiltModel_Destroy.
// - omg input order = topological first-use order; callers validate
//   dims/byte sizes instead of trusting tensor names.
#[cfg(target_env = "ohos")]
pub mod om {
    use std::ffi::{CStr, CString, c_char, c_int, c_void};
    use std::path::Path;
    use std::sync::OnceLock;

    use anyhow::Result;

    const VENDOR_LIB_DIR: &str = "/vendor/lib64/passthrough/indirect";
    const RTLD_NOW: c_int = 0x2;
    const RTLD_LOCAL: c_int = 0x0;
    /// Harness parity: refuse absurd tensor counts.
    const MAX_TENSORS: usize = 16;

    // OHOS libc (musl-based, like bionic: dlopen/dlsym live in libc itself).
    unsafe extern "C" {
        fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlerror() -> *const c_char;
    }

    /// Function-pointer table for the three vendor facades. Signatures were
    /// decoded during the campaign (see module docs); `*mut c_void` stands
    /// for the corresponding opaque HIAI handle.
    struct OmApi {
        // libai_fmk_model_runtime_core.so
        restore_from_file: unsafe extern "C" fn(*const c_char) -> *mut c_void,
        built_model_destroy: unsafe extern "C" fn(*mut c_void),
        get_input_tensor_num: unsafe extern "C" fn(*mut c_void) -> c_int,
        get_input_tensor_desc: unsafe extern "C" fn(*mut c_void, usize) -> *mut c_void,
        get_input_tensor_name:
            unsafe extern "C" fn(*mut c_void, usize, *mut c_char, usize) -> c_int,
        get_output_tensor_num: unsafe extern "C" fn(*mut c_void) -> c_int,
        get_output_tensor_desc: unsafe extern "C" fn(*mut c_void, usize) -> *mut c_void,
        get_output_tensor_name:
            unsafe extern "C" fn(*mut c_void, usize, *mut c_char, usize) -> c_int,
        options_create: unsafe extern "C" fn() -> *mut c_void,
        options_destroy: unsafe extern "C" fn(*mut *mut c_void),
        manager_create: unsafe extern "C" fn() -> *mut c_void,
        manager_destroy: unsafe extern "C" fn(*mut c_void),
        manager_init:
            unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const c_void) -> c_int,
        manager_run: unsafe extern "C" fn(
            *mut c_void,
            *mut *mut c_void,
            usize,
            *mut *mut c_void,
            usize,
        ) -> c_int,
        manager_deinit: unsafe extern "C" fn(*mut c_void) -> c_int,
        // libai_fmk_tensor.so
        desc_get_dim: unsafe extern "C" fn(*mut c_void, usize) -> i32,
        desc_get_byte_size: unsafe extern "C" fn(*mut c_void) -> u64,
        // libai_fmk_hcl_model_runtime.so (facade)
        mr_buffer_create: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
        mr_buffer_get_data: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
        mr_buffer_get_size: unsafe extern "C" fn(*mut c_void) -> u64,
        mr_buffer_get_fd: unsafe extern "C" fn(*mut c_void) -> c_int,
        mr_buffer_destroy: unsafe extern "C" fn(*mut *mut c_void),
        set_perf_mode: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
        get_perf_mode: unsafe extern "C" fn(*mut c_void) -> c_int,
    }

    static API: OnceLock<OmApi> = OnceLock::new();

    fn api() -> Result<&'static OmApi> {
        if let Some(loaded) = API.get() {
            return Ok(loaded);
        }
        // Single-threaded by construction: only reached through this
        // `OnceLock` gate, so at most one caller performs the dlopen/dlsym
        // dance.
        let loaded = load_api()?;
        let _ = API.set(loaded);
        Ok(API.get().expect("OmApi was just installed"))
    }

    /// Resolve the vendor function table once per process.
    fn load_api() -> Result<OmApi> {
        // Dependency order: tensor → core → HCL facade (each dlopen
        // registers its soname, satisfying the next lib's DT_NEEDED
        // without the vendor dir having to be on the default search path).
        let tensor = dlopen_lib("libai_fmk_tensor.so")?;
        let core = dlopen_lib("libai_fmk_model_runtime_core.so")?;
        let hcl = dlopen_lib("libai_fmk_hcl_model_runtime.so")?;

        Ok(OmApi {
            restore_from_file: sym_fn(core, "HIAI_MR_BuiltModel_RestoreFromFile")?,
            built_model_destroy: sym_fn(core, "HIAI_MR_BuiltModel_Destroy")?,
            get_input_tensor_num: sym_fn(core, "HIAI_MR_BuiltModel_GetInputTensorNum")?,
            get_input_tensor_desc: sym_fn(core, "HIAI_MR_BuiltModel_GetInputTensorDesc")?,
            get_input_tensor_name: sym_fn(core, "HIAI_MR_BuiltModel_GetInputTensorName")?,
            get_output_tensor_num: sym_fn(core, "HIAI_MR_BuiltModel_GetOutputTensorNum")?,
            get_output_tensor_desc: sym_fn(core, "HIAI_MR_BuiltModel_GetOutputTensorDesc")?,
            get_output_tensor_name: sym_fn(core, "HIAI_MR_BuiltModel_GetOutputTensorName")?,
            options_create: sym_fn(core, "HIAI_MR_ModelInitOptions_Create")?,
            options_destroy: sym_fn(core, "HIAI_MR_ModelInitOptions_Destroy")?,
            manager_create: sym_fn(core, "HIAI_MR_ModelManager_Create")?,
            manager_destroy: sym_fn(core, "HIAI_MR_ModelManager_Destroy")?,
            manager_init: sym_fn(core, "HIAI_MR_ModelManager_Init")?,
            manager_run: sym_fn(core, "HIAI_MR_ModelManager_Run")?,
            manager_deinit: sym_fn(core, "HIAI_MR_ModelManager_Deinit")?,
            desc_get_dim: sym_fn(tensor, "HIAI_NDTensorDesc_GetDim")?,
            desc_get_byte_size: sym_fn(tensor, "HIAI_NDTensorDesc_GetByteSize")?,
            mr_buffer_create: sym_fn(hcl, "HIAI_MR_NDTensorBuffer_CreateFromNDTensorDesc")?,
            mr_buffer_get_data: sym_fn(hcl, "HIAI_MR_NDTensorBuffer_GetData")?,
            mr_buffer_get_size: sym_fn(hcl, "HIAI_MR_NDTensorBuffer_GetSize")?,
            mr_buffer_get_fd: sym_fn(hcl, "HIAI_MR_NDTensorBuffer_GetFd")?,
            mr_buffer_destroy: sym_fn(hcl, "HIAI_MR_NDTensorBuffer_Destroy")?,
            set_perf_mode: sym_fn(hcl, "HIAI_HCL_ModelInitOptions_SetPerfMode")?,
            get_perf_mode: sym_fn(hcl, "HIAI_HCL_ModelInitOptions_GetPerfMode")?,
        })
    }

    /// dlopen a vendor library by absolute path first, bare soname second.
    fn dlopen_lib(soname: &str) -> Result<*mut c_void> {
        let candidates = [format!("{VENDOR_LIB_DIR}/{soname}"), soname.to_string()];
        let mut last_error = String::new();
        for candidate in &candidates {
            let c_name = CString::new(candidate.as_str())
                .map_err(|e| anyhow::anyhow!("library name CString conversion failed: {e}"))?;
            // SAFETY: c_name is a valid NUL-terminated string; the flags are
            // plain integer constants.
            let handle = unsafe { dlopen(c_name.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
            if !handle.is_null() {
                return Ok(handle);
            }
            last_error = dl_error_message();
        }
        anyhow::bail!("dlopen {soname} failed: {last_error}")
    }

    /// Resolve a symbol address, failing with the pending dlerror message.
    fn sym(handle: *mut c_void, name: &str) -> Result<*mut c_void> {
        let c_name = CString::new(name)
            .map_err(|e| anyhow::anyhow!("symbol name CString conversion failed: {e}"))?;
        // SAFETY: handle came from a successful dlopen; c_name is a valid
        // NUL-terminated symbol name.
        let address = unsafe { dlsym(handle, c_name.as_ptr()) };
        if address.is_null() {
            anyhow::bail!("dlsym {name} failed: {}", dl_error_message());
        }
        Ok(address)
    }

    /// Resolve a typed function pointer by symbol name. Private on
    /// purpose: `T` is always inferred from an `OmApi` field, whose types
    /// mirror the decoded vendor signatures (see the campaign harness).
    fn sym_fn<T>(handle: *mut c_void, name: &str) -> Result<T> {
        let address = sym(handle, name)?;
        // SAFETY: dlsym returned the function's address; T is a fn pointer
        // of pointer size matching the documented ABI (pinned by the
        // private OmApi-field call sites).
        Ok(unsafe { std::mem::transmute_copy(&address) })
    }

    fn dl_error_message() -> String {
        // SAFETY: dlerror returns a thread-local NUL-terminated string (or
        // null when no error is pending).
        let err = unsafe { dlerror() };
        if err.is_null() {
            "unknown dlopen/dlsym error".to_string()
        } else {
            // SAFETY: err is a valid NUL-terminated C string.
            unsafe { CStr::from_ptr(err) }
                .to_string_lossy()
                .into_owned()
        }
    }

    /// One model input or output: its ION-backed facade buffer, the cached
    /// data pointer, and the declared shape.
    struct IoSlot {
        buf: *mut c_void,
        data: *mut u8,
        byte_size: usize,
        dims: Vec<i32>,
        name: String,
    }

    /// Shape metadata of one model input/output, for contract validation at
    /// the model-construction sites.
    #[derive(Debug, Clone)]
    pub struct IoInfo {
        pub name: String,
        pub dims: Vec<i32>,
        pub byte_size: usize,
    }

    /// Safe wrapper around a restored HIAI_MR model. Holds the raw handles
    /// plus the cached ION buffer slots; `write_input_f32` → `run` →
    /// `read_output_f32` reuses the same buffers across calls.
    ///
    /// Like `ms_lite::MsModel`, instances live behind a `Mutex` (the vendor
    /// API is not thread-safe per handle). `Drop` performs the
    /// harness-proven teardown; a partially-initialized model (constructor
    /// failure) drops cleanly because every teardown step is guarded.
    pub struct OmModel {
        api: &'static OmApi,
        model: *mut c_void,
        mgr: *mut c_void,
        opts: *mut c_void,
        input_slots: Vec<IoSlot>,
        output_slots: Vec<IoSlot>,
    }

    // SAFETY: OmModel is only accessed behind `Mutex<OmModel>` (via
    // `DenoiseSession::Cann` in ai_processing.rs);
    // predict calls are strictly serialized by the owning Mutex.
    unsafe impl Send for OmModel {}
    unsafe impl Sync for OmModel {}

    impl OmModel {
        /// Restore an `.om` model and prepare it for NPU execution.
        ///
        /// `perf_mode` follows the HIAI_HCL perf-mode domain (0-4, vendor
        /// default 2); pass 4 for small-op-dense CNN graphs (the §8.4
        /// measured optimum for NIND) or a negative value to keep the
        /// default.
        pub fn new(model_path: &Path, perf_mode: i32) -> Result<Self> {
            let api = api()?;
            let c_path = CString::new(model_path.to_string_lossy().into_owned())
                .map_err(|e| anyhow::anyhow!("model path CString conversion failed: {e}"))?;

            // SAFETY: c_path is a valid NUL-terminated path string.
            let model = unsafe { (api.restore_from_file)(c_path.as_ptr()) };
            if model.is_null() {
                anyhow::bail!(
                    "BuiltModel_RestoreFromFile returned null (corrupt .om or unsupported platform?)"
                );
            }

            // Partially initialized on purpose: every subsequent step can
            // fail via `?` and Drop tears down whatever was created.
            let mut m = OmModel {
                api,
                model,
                mgr: std::ptr::null_mut(),
                opts: std::ptr::null_mut(),
                input_slots: Vec::new(),
                output_slots: Vec::new(),
            };
            m.init_options(perf_mode)?;
            m.init_manager()?;
            m.create_io_slots()?;
            log::info!(
                "CANN .om model ready ({} input(s), {} output(s), perf_mode requested {perf_mode})",
                m.input_slots.len(),
                m.output_slots.len()
            );
            Ok(m)
        }

        fn init_options(&mut self, perf_mode: i32) -> Result<()> {
            // SAFETY: plain allocator call.
            let opts = unsafe { (self.api.options_create)() };
            if opts.is_null() {
                anyhow::bail!("ModelInitOptions_Create returned null");
            }
            self.opts = opts;

            if perf_mode >= 0 {
                // SAFETY: opts was just created. The setter's return value
                // is garbage (harness-proven) — the applied mode is verified
                // through the Get accessor instead.
                unsafe {
                    let _ = (self.api.set_perf_mode)(self.opts, perf_mode);
                    let applied = (self.api.get_perf_mode)(self.opts);
                    if applied != perf_mode {
                        log::warn!(
                            "perf mode {perf_mode} requested, {applied} reported — continuing"
                        );
                    }
                }
            }
            Ok(())
        }

        fn init_manager(&mut self) -> Result<()> {
            // SAFETY: plain allocator call.
            let mgr = unsafe { (self.api.manager_create)() };
            if mgr.is_null() {
                anyhow::bail!("ModelManager_Create returned null");
            }
            self.mgr = mgr;

            // SAFETY: mgr/opts/model were all created above; extra=NULL is
            // the harness-proven call shape.
            let rc = unsafe {
                (self.api.manager_init)(self.mgr, self.opts, self.model, std::ptr::null())
            };
            if rc != 0 {
                anyhow::bail!("ModelManager_Init rc={rc}");
            }
            Ok(())
        }

        fn create_io_slots(&mut self) -> Result<()> {
            // SAFETY: model handle was validated in new().
            let (n_in, n_out) = unsafe {
                (
                    (self.api.get_input_tensor_num)(self.model),
                    (self.api.get_output_tensor_num)(self.model),
                )
            };
            if n_in <= 0
                || n_out <= 0
                || n_in as usize > MAX_TENSORS
                || n_out as usize > MAX_TENSORS
            {
                anyhow::bail!("unexpected IO tensor counts (in={n_in}, out={n_out})");
            }
            for idx in 0..n_in as usize {
                // SAFETY: idx < n_in <= MAX_TENSORS, as returned by the
                // model; self.model is a restored BuiltModel.
                let slot = unsafe { self.make_slot(true, idx)? };
                self.input_slots.push(slot);
            }
            for idx in 0..n_out as usize {
                // SAFETY: idx < n_out <= MAX_TENSORS, as returned by the
                // model; self.model is a restored BuiltModel.
                let slot = unsafe { self.make_slot(false, idx)? };
                self.output_slots.push(slot);
            }
            Ok(())
        }

        /// Create the ION-backed facade buffer for one input/output.
        ///
        /// # Safety
        /// `idx` must be below the model's declared tensor count for the
        /// chosen side, and `self.model` must be a restored BuiltModel.
        unsafe fn make_slot(&self, is_input: bool, idx: usize) -> Result<IoSlot> {
            let api = self.api;
            // SAFETY: caller guarantees idx is in range and the model handle
            // is valid.
            let (desc, name) = unsafe {
                let desc = if is_input {
                    (api.get_input_tensor_desc)(self.model, idx)
                } else {
                    (api.get_output_tensor_desc)(self.model, idx)
                };
                if desc.is_null() {
                    anyhow::bail!(
                        "Get{}TensorDesc({idx}) returned null",
                        if is_input { "Input" } else { "Output" }
                    );
                }

                let mut name_buf = [0u8; 256];
                let name_rc = if is_input {
                    (api.get_input_tensor_name)(
                        self.model,
                        idx,
                        name_buf.as_mut_ptr().cast::<c_char>(),
                        name_buf.len(),
                    )
                } else {
                    (api.get_output_tensor_name)(
                        self.model,
                        idx,
                        name_buf.as_mut_ptr().cast::<c_char>(),
                        name_buf.len(),
                    )
                };
                let name = if name_rc == 0 {
                    // SAFETY: on rc==0 the API wrote a NUL-terminated name
                    // into name_buf.
                    CStr::from_bytes_until_nul(&name_buf)
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|_| format!("io{idx}"))
                } else {
                    format!("<name rc={name_rc}>")
                };
                (desc, name)
            };

            // SAFETY: desc is a valid tensor descriptor owned by the model.
            let (dims, byte_size) = unsafe {
                let dims: Vec<i32> = (0..8)
                    .map(|k| (api.desc_get_dim)(desc, k))
                    .take_while(|&d| d >= 0)
                    .collect();
                let byte_size = (api.desc_get_byte_size)(desc) as usize;
                (dims, byte_size)
            };

            // ION-backed facade buffer — plain tensor.so buffers SIGSEGV
            // inside Run (RunV3 requires ION/DMA backing).
            // SAFETY: desc is valid; the returned buffer is owned by us.
            let buf = unsafe { (api.mr_buffer_create)(desc) };
            if buf.is_null() {
                anyhow::bail!("NDTensorBuffer_CreateFromNDTensorDesc('{name}') returned null");
            }

            // fd sanity gate (harness-proven): fd < 0 means the buffer is
            // not ION-backed and Run would SIGSEGV — refuse it.
            // SAFETY: buf was just created.
            let fd = unsafe { (api.mr_buffer_get_fd)(buf) };
            if fd < 0 {
                // SAFETY: buf is a live buffer; destroy takes the address of
                // the handle.
                let mut b = buf;
                unsafe { (api.mr_buffer_destroy)(&mut b) };
                anyhow::bail!("buffer '{name}' is not ION-backed (fd={fd}) — refusing to Run");
            }

            // SAFETY: buf is a live ION-backed buffer.
            let (data, buffer_bytes) = unsafe {
                (
                    (api.mr_buffer_get_data)(buf) as *mut u8,
                    (api.mr_buffer_get_size)(buf) as usize,
                )
            };
            if data.is_null() || byte_size == 0 || buffer_bytes < byte_size {
                // SAFETY: buf is a live buffer; destroy takes the address of
                // the handle.
                let mut b = buf;
                unsafe { (api.mr_buffer_destroy)(&mut b) };
                anyhow::bail!(
                    "bad buffer '{name}' (data={data:?}, desc_bytes={byte_size}, buffer_bytes={buffer_bytes})"
                );
            }

            log::debug!(
                "om {}[{}] '{}' dims={dims:?} bytes={byte_size} fd={fd}",
                if is_input { "in" } else { "out" },
                idx,
                name
            );

            Ok(IoSlot {
                buf,
                data,
                byte_size,
                dims,
                name,
            })
        }

        pub fn input_count(&self) -> usize {
            self.input_slots.len()
        }

        pub fn output_count(&self) -> usize {
            self.output_slots.len()
        }

        pub fn input_info(&self, idx: usize) -> Option<IoInfo> {
            self.input_slots.get(idx).map(|s| IoInfo {
                name: s.name.clone(),
                dims: s.dims.clone(),
                byte_size: s.byte_size,
            })
        }

        pub fn output_info(&self, idx: usize) -> Option<IoInfo> {
            self.output_slots.get(idx).map(|s| IoInfo {
                name: s.name.clone(),
                dims: s.dims.clone(),
                byte_size: s.byte_size,
            })
        }

        /// Copy f32 values into input `idx`'s ION buffer.
        pub fn write_input_f32(&self, idx: usize, values: &[f32]) -> Result<()> {
            let Some(slot) = self.input_slots.get(idx) else {
                anyhow::bail!(
                    "no input tensor {idx} (model has {})",
                    self.input_slots.len()
                );
            };
            let expected = slot.byte_size / std::mem::size_of::<f32>();
            if values.len() != expected {
                anyhow::bail!(
                    "input {idx} ('{}'): got {} f32 values, expected {expected}",
                    slot.name,
                    values.len()
                );
            }
            // SAFETY: slot.data points at the buffer's writable data region
            // of byte_size bytes (validated at init); the owning Mutex
            // serializes access.
            unsafe {
                std::ptr::copy_nonoverlapping(values.as_ptr(), slot.data.cast::<f32>(), expected);
            };
            Ok(())
        }

        /// Execute the model. Inputs must have been filled via
        /// `write_input_f32`; afterwards outputs are read with
        /// `read_output_f32`.
        pub fn run(&self) -> Result<()> {
            let mut in_ptrs: Vec<*mut c_void> = self.input_slots.iter().map(|s| s.buf).collect();
            let mut out_ptrs: Vec<*mut c_void> = self.output_slots.iter().map(|s| s.buf).collect();
            // SAFETY: mgr was Init'ed; the pointer arrays match the slot
            // buffers and counts (harness-proven call shape).
            let rc = unsafe {
                (self.api.manager_run)(
                    self.mgr,
                    in_ptrs.as_mut_ptr(),
                    in_ptrs.len(),
                    out_ptrs.as_mut_ptr(),
                    out_ptrs.len(),
                )
            };
            if rc != 0 {
                anyhow::bail!("ModelManager_Run rc={rc}");
            }
            Ok(())
        }

        /// Copy f32 values out of output `idx`'s ION buffer.
        pub fn read_output_f32(&self, idx: usize) -> Result<Vec<f32>> {
            let Some(slot) = self.output_slots.get(idx) else {
                anyhow::bail!(
                    "no output tensor {idx} (model has {})",
                    self.output_slots.len()
                );
            };
            let n = slot.byte_size / std::mem::size_of::<f32>();
            let mut out = vec![0f32; n];
            // SAFETY: slot.data points at the buffer's readable data region
            // filled by the last successful run().
            unsafe {
                std::ptr::copy_nonoverlapping(slot.data.cast::<f32>(), out.as_mut_ptr(), n);
            };
            Ok(out)
        }
    }

    impl Drop for OmModel {
        fn drop(&mut self) {
            // Harness-proven teardown order, guarded so a
            // partially-initialized model also drops cleanly: buffers →
            // ModelManager_Deinit/Destroy → ModelInitOptions_Destroy →
            // BuiltModel_Destroy.
            for slot in self.input_slots.drain(..) {
                // SAFETY: slot.buf is a live facade buffer created by
                // make_slot; destroy takes the address of the handle.
                let mut b = slot.buf;
                unsafe { (self.api.mr_buffer_destroy)(&mut b) };
            }
            for slot in self.output_slots.drain(..) {
                // SAFETY: see above.
                let mut b = slot.buf;
                unsafe { (self.api.mr_buffer_destroy)(&mut b) };
            }
            if !self.mgr.is_null() {
                // SAFETY: mgr was Init'ed (or at least created); the
                // harness-proven sequence is Deinit before Destroy.
                unsafe {
                    let _ = (self.api.manager_deinit)(self.mgr);
                    (self.api.manager_destroy)(self.mgr);
                }
                self.mgr = std::ptr::null_mut();
            }
            if !self.opts.is_null() {
                // SAFETY: opts was created in init_options; destroy takes
                // the address of the handle.
                let mut o = self.opts;
                unsafe { (self.api.options_destroy)(&mut o) };
                self.opts = std::ptr::null_mut();
            }
            if !self.model.is_null() {
                // SAFETY: model is a restored BuiltModel.
                unsafe { (self.api.built_model_destroy)(self.model) };
                self.model = std::ptr::null_mut();
            }
        }
    }
}
