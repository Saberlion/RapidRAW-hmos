import { DefaultXComponent } from '@ohos-rs/ability';

// Custom @Entry page (machine-local patch — docs/HARMONYOS_PORTING.md §6.5):
// the crate's own MainPage (0.4.0-beta.0) has no back-gesture handling, and
// ability-level onBackPressed can only background or destroy the app — it
// would fire even when the webview just wanted to close a modal. This page
// keeps the stock DefaultXComponent layout (webview hosted by the Rust
// render call) and fully consumes the back gesture at page level, forwarding
// it to the Rust bridge; the webview then runs the same chain as the Android
// hardware back button (useAndroidBackHandler.ts).
@Entry
@Component
struct Index {
  private nativeModule: ESObject | null = null;

  async aboutToAppear(): Promise<void> {
    // Must be a variable: a literal module specifier fails ArkTS resolution
    // (docs 6.5).
    const libName: string = 'librapidraw_lib.so';
    try {
      this.nativeModule = await import(libName);
    } catch (e) {
      console.error(`[RapidRAW] back-press page module load failed: ${JSON.stringify(e)}`);
    }
  }

  onBackPress(): boolean {
    const notify: ESObject =
      this.nativeModule?.notifyOhosBackPressed ?? this.nativeModule?.default?.notifyOhosBackPressed;
    if (typeof notify === 'function') {
      notify();
      return true;
    }
    return false;
  }

  build() {
    Row() {
      Column() {
        DefaultXComponent()
      }.width('100%')
    }.height('100%')
  }
}
