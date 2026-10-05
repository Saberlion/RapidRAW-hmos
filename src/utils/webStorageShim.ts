/**
 * ArkWeb (the OpenHarmony webview) exposes `window.localStorage` as null —
 * any access (even `Object.keys(localStorage)`) throws a TypeError. RapidRAW
 * itself does not use web storage (all persistence goes through the Rust
 * settings sidecar), but this guard keeps future code and third-party
 * libraries from crashing on OHOS. When a storage is missing or throws on
 * access, an in-memory replacement is installed; values are NOT persisted
 * across restarts. See docs/HARMONYOS_PORTING.md 6.13.
 */

function createMemoryStorage(): Storage {
  const map = new Map<string, string>();

  return {
    get length() {
      return map.size;
    },
    clear: () => map.clear(),
    getItem: (key: string) => map.get(key) ?? null,
    key: (index: number) => Array.from(map.keys())[index] ?? null,
    removeItem: (key: string) => {
      map.delete(key);
    },
    setItem: (key: string, value: string) => {
      map.set(String(key), String(value));
    },
  };
}

export function installWebStorageShim(): void {
  (['localStorage', 'sessionStorage'] as const).forEach((name) => {
    let accessible = false;
    try {
      const storage = window[name];
      accessible = !!storage && typeof storage.setItem === 'function';
    } catch {
      // Accessing the property threw — treat as inaccessible (stays false).
    }

    if (!accessible) {
      try {
        Object.defineProperty(window, name, {
          configurable: true,
          value: createMemoryStorage(),
        });
        console.warn(
          `[web-storage] ${name} is unavailable in this webview; installed a non-persistent in-memory fallback`,
        );
      } catch {
        // window is sealed — nothing else we can do
      }
    }
  });
}
