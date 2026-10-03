//! OpenHarmony (OHOS) platform integration.
//!
//! Mirrors `android_integration.rs`: this module hosts all OpenHarmony-specific
//! glue code. It is only compiled for `*-unknown-linux-ohos` targets, which
//! report `target_os = "linux"` + `target_env = "ohos"`.
//!
//! Porting guide & progress tracker: `docs/HARMONYOS_PORTING.md`.

#[cfg(target_env = "ohos")]
pub fn initialize_ohos(window: &tauri::WebviewWindow) {
    log::info!(
        "Initializing RapidRAW on OpenHarmony (window: '{}').",
        window.label()
    );

    // Phase 2 work items (see docs/HARMONYOS_PORTING.md):
    // - User file access via FileKit / photoAccessHelper URIs (the equivalent
    //   of the Android content-URI bridge), wired through the tauri-ohos NAPI
    //   layer (`@ohos-rs/ability`).
    // - Gallery export (`save_image_bytes_to_ohos_gallery`) as the equivalent
    //   of `save_image_bytes_to_android_gallery`.
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

    /// Forward a window operation to the ArkTS side.
    pub fn dispatch(op: &str) -> std::result::Result<(), String> {
        let guard = CONTROLLER.lock().unwrap();
        match guard.as_ref() {
            Some(controller) => {
                let status =
                    controller.call(op.to_string(), ThreadsafeFunctionCallMode::NonBlocking);
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
}

/// Whether this build targets OpenHarmony. The frontend uses this to route
/// titlebar window controls around the tao-ohos stubs (`platform()` from
/// tauri-plugin-os reports "linux" on OHOS, so it cannot be detected that way).
#[tauri::command]
pub fn is_ohos_build() -> bool {
    cfg!(target_env = "ohos")
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
