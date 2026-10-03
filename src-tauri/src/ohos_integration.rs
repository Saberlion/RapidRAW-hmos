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
