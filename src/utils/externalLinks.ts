import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-shell';
import { Invokes } from '../components/ui/AppProperties';
import { useSettingsStore } from '../store/useSettingsStore';

/**
 * Schemes an external link may use. Mirrors the list tauri-plugin-shell's
 * injected init script accepts, so routing a link through the app here gives it
 * no reach it did not already have.
 */
const EXTERNAL_SCHEME = /^(https?|mailto|tel):/i;

/**
 * Open an external URL in the system browser.
 *
 * On OpenHarmony this must not go through `@tauri-apps/plugin-shell`'s `open`:
 * OHOS is neither `android` nor `ios`, so the plugin compiles its desktop
 * (Linux) path and spawns `xdg-open`, which does not exist on HarmonyOS —
 * every external link click ended as
 * `Unhandled promise rejection Scoped shell IO error: No such file or
 * directory (os error 2)` and nothing opened. OHOS instead goes through the
 * `open_external_url` command, which hands the URL to the ArkTS bridge and
 * opens it with a `viewData` want (docs 6.19). Other platforms keep the plugin.
 */
export async function openExternalUrl(url: string): Promise<void> {
  if (!EXTERNAL_SCHEME.test(url)) {
    return;
  }

  try {
    if (useSettingsStore.getState().isOhos) {
      await invoke(Invokes.OpenExternalUrl, { url });
      return;
    }

    await open(url);
  } catch (err) {
    // Fire-and-forget by design (a link click must not surface an unhandled
    // rejection); the log bridge forwards this to app.log.
    console.error('Failed to open external URL', url, err);
  }
}

/**
 * Claim clicks on external `<a target="_blank">` links on OHOS.
 *
 * tauri-plugin-shell injects a document-start script (`src/init-iife.js`) that
 * listens on `document.body` for clicks on `<a target="_blank">` whose href
 * starts with `http://`, `https://`, `mailto:` or `tel:`, calls
 * `plugin:shell|open` for them and `preventDefault()`s the event. On OHOS that
 * call always fails, so the link neither opens externally nor navigates.
 *
 * This listener runs in the capture phase and stops propagation for exactly
 * those links, so the plugin's bubble-phase handler never sees the event, and
 * opens the URL through {@link openExternalUrl} instead. It acts only when the
 * platform probe reports OHOS, leaving desktop behaviour untouched.
 */
export function installExternalLinkHandler(): void {
  document.addEventListener(
    'click',
    (event) => {
      if (event.defaultPrevented || event.button !== 0) {
        return;
      }
      if (!useSettingsStore.getState().isOhos) {
        return;
      }

      const target = event.target;
      if (!(target instanceof Element)) {
        return;
      }
      const anchor = target.closest('a');
      if (!(anchor instanceof HTMLAnchorElement)) {
        return;
      }
      if (anchor.target !== '_blank' || !EXTERNAL_SCHEME.test(anchor.getAttribute('href') ?? '')) {
        return;
      }

      event.preventDefault();
      event.stopPropagation();
      void openExternalUrl(anchor.href);
    },
    true,
  );
}
