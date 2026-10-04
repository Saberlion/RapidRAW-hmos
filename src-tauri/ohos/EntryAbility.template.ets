import { RustAbility } from '@ohos-rs/ability'
import Want from '@ohos.app.ability.Want'
import { AbilityConstant, Configuration, ConfigurationConstant } from '@kit.AbilityKit';
import { BusinessError } from '@kit.BasicServicesKit';
import window from '@ohos.window';
import picker from '@ohos.file.picker';
import fileUri from '@ohos.file.fileuri';
import fs from '@ohos.file.fs';
import { photoAccessHelper } from '@kit.MediaLibraryKit';

export default class EntryAbility extends RustAbility {
  // change to dynamic library name
  public moduleName: string = "rapidraw_lib"
  // Load our own pages/Index instead of the crate's MainPage: the custom
  // page adds the page-level onBackPress bridge (docs 6.5). The webview is
  // still created by DefaultXComponent inside that page.
  public defaultPage: boolean = false;
  public mode: 'xcomponent' | 'webview' = 'webview'
  private rapidrawModule: ESObject | null = null;

  async onCreate(want: Want, launchParam: AbilityConstant.LaunchParam): Promise<void> {
    super.onCreate(want, launchParam);
  }

  async onWindowStageCreate(windowStage: window.WindowStage): Promise<void> {
    const window = windowStage.getMainWindowSync();
    await window.setWindowLayoutFullScreen(false);
    // Deterministic keyboard avoidance: RESIZE shrinks the window so the
    // webview's visualViewport handlers reflow the UI above the IME, the
    // same effect as the Android insets padding in MainActivity.kt.
    try {
      window.setKeyboardAvoidMode(window.KeyboardAvoidMode.RESIZE);
    } catch (e) {
      console.error(`[RapidRAW] setKeyboardAvoidMode failed: ${JSON.stringify(e)}`);
    }
    // Safe-area evidence: with layoutFullScreen(false) the window manager
    // reserves the system bars already; log the remaining avoid rect.
    try {
      const avoid = window.getWindowAvoidArea(window.AvoidAreaType.TYPE_SYSTEM);
      console.info(`[RapidRAW] system avoid area: ${JSON.stringify(avoid)}`);
    } catch (e) {
      console.error(`[RapidRAW] getWindowAvoidArea failed: ${JSON.stringify(e)}`);
    }
    // RapidRAW's custom titlebar drives window controls through tauri
    // commands into ohos_integration.rs, which calls back into the JS
    // callback registered here (tao-ohos stubs the native window ops).
    // Fire-and-forget on purpose: awaiting lifecycle helpers here once
    // caused the ability to terminate right after startup (docs 6.4).
    this.registerRapidrawWindowController(window);
    super.onWindowStageCreate(windowStage);
    await windowStage.loadContent('pages/Index');
    // Keep the system window decoration VISIBLE. We previously hid it
    // (setWindowDecorVisible(false) retry loop) so the custom webview
    // titlebar could own the whole window, but that is unsafe: hiding the
    // decor only removes the visuals — its window-button INPUT rects stay
    // active inside the content area, offset ~one button left of where our
    // webview buttons render, so taps got misrouted to the wrong system op
    // (tapping our maximize icon closed the app). With the decor visible the
    // rects live in the system bar where they belong; the frontend hides its
    // own button cluster on OHOS (TitleBar.tsx) and window dragging goes
    // through the startMoving bridge. Machine-local post-init patch — see
    // docs/HARMONYOS_PORTING.md 6.4.
  }

  // Machine-local post-init patch — see docs/HARMONYOS_PORTING.md 6.4.
  private async registerRapidrawWindowController(win: window.Window): Promise<void> {
    try {
      const libName: string = 'librapidraw_lib.so';
      const mod: ESObject = await import(libName);
      const register: ESObject =
        mod.registerOhosWindowController ?? mod.default?.registerOhosWindowController;
      if (typeof register !== 'function') {
        console.error('[RapidRAW] registerOhosWindowController not exported by librapidraw_lib.so');
        return;
      }
      const resolvePickFolder: ESObject =
        mod.resolveOhosPickFolder ?? mod.default?.resolveOhosPickFolder;
      const resolvePickFiles: ESObject =
        mod.resolveOhosPickFiles ?? mod.default?.resolveOhosPickFiles;
      const resolveSaveToGallery: ESObject =
        mod.resolveOhosSaveToGallery ?? mod.default?.resolveOhosSaveToGallery;
      const resolveSaveFileAs: ESObject =
        mod.resolveOhosSaveFileAs ?? mod.default?.resolveOhosSaveFileAs;
      if (typeof resolvePickFolder !== 'function' || typeof resolvePickFiles !== 'function' ||
        typeof resolveSaveToGallery !== 'function' || typeof resolveSaveFileAs !== 'function') {
        console.error('[RapidRAW] bridge resolve exports missing from librapidraw_lib.so');
      }
      register((op: string) => {
        try {
          // JSON payloads ({ op, requestId, ... }) route to the bridge
          // handlers; plain op strings are the legacy window controls.
          let req: Record<string, Object> | null = null;
          try {
            req = JSON.parse(op) as Record<string, Object>;
          } catch (e) {
            req = null;
          }
          if (req !== null && typeof req['op'] === 'string') {
            this.handleBridgeRequest(req, resolvePickFolder, resolvePickFiles, resolveSaveToGallery, resolveSaveFileAs);
            return;
          }
          switch (op) {
            case 'minimize':
              win.minimize();
              break;
            case 'maximize':
              win.maximize().catch((e: BusinessError) => {
                console.error(`[RapidRAW] window op 'maximize' failed: ${JSON.stringify(e)}`);
              });
              break;
            case 'restore':
              win.restore();
              break;
            case 'start_drag':
              win.startMoving();
              break;
            default:
              console.warn(`[RapidRAW] unknown window op '${op}'`);
          }
        } catch (e) {
          console.error(`[RapidRAW] window op '${op}' failed: ${JSON.stringify(e)}`);
        }
      });
      // Keep the module handle for onConfigurationUpdate pushes, and seed
      // the Rust bridge with the current system color mode.
      this.rapidrawModule = mod;
      this.pushColorMode(this.context.config?.colorMode);
      console.info('[RapidRAW] OHOS window controller registered');
    } catch (e) {
      console.error(`[RapidRAW] window-controller registration failed: ${JSON.stringify(e)}`);
    }
  }

  onConfigurationUpdate(newConfig: Configuration): void {
    super.onConfigurationUpdate(newConfig);
    this.pushColorMode(newConfig.colorMode);
  }

  // Forward the system color mode into the Rust bridge; the webview receives
  // changes as the 'ohos-color-mode' tauri event (see ohos_integration.rs).
  private pushColorMode(colorMode: number | undefined): void {
    const notify: ESObject =
      this.rapidrawModule?.notifyOhosColorMode ?? this.rapidrawModule?.default?.notifyOhosColorMode;
    if (typeof notify !== 'function') {
      return;
    }
    if (colorMode === ConfigurationConstant.ColorMode.COLOR_DARK) {
      notify(true);
    } else if (colorMode === ConfigurationConstant.ColorMode.COLOR_LIGHT) {
      notify(false);
    }
  }

  // Every handler MUST resolve its request exactly once — the Rust side parks
  // a oneshot channel per request id and has no timeout.
  private handleBridgeRequest(
    req: Record<string, Object>,
    resolvePickFolder: ESObject,
    resolvePickFiles: ESObject,
    resolveSaveToGallery: ESObject,
    resolveSaveFileAs: ESObject,
  ): void {
    const op: string = req['op'] as string;
    const requestId: number = req['requestId'] as number;
    switch (op) {
      case 'pick_folder':
        this.pickFolder(requestId, resolvePickFolder);
        break;
      case 'pick_files':
        this.pickFiles(req, requestId, resolvePickFiles);
        break;
      case 'save_to_gallery':
        this.saveToGallery(req, requestId, resolveSaveToGallery);
        break;
      case 'save_file_as':
        this.saveFileAs(req, requestId, resolveSaveFileAs);
        break;
      default:
        console.warn(`[RapidRAW] unknown bridge op '${op}'`);
    }
  }

  private pickFolder(requestId: number, resolvePickFolder: ESObject): void {
    const options: picker.DocumentSelectOptions = new picker.DocumentSelectOptions();
    options.selectMode = picker.DocumentSelectMode.FOLDER;
    options.maxSelectNumber = 1;
    const documentPicker = new picker.DocumentViewPicker(this.context);
    documentPicker.select(options).then((uris: Array<string>) => {
      let resolved: string | null = null;
      if (uris && uris.length > 0) {
        const rawUri: string = uris[0];
        console.info(`[RapidRAW] pick_folder raw uri: ${rawUri}`);
        try {
          // DocumentViewPicker returns file://docs/... URIs; FileUri.path
          // yields the app-sandbox path directly (verified: readable by the
          // Rust side, same process).
          resolved = new fileUri.FileUri(rawUri).path || null;
          console.info(`[RapidRAW] pick_folder path: ${resolved}`);
        } catch (e) {
          console.error(`[RapidRAW] folder URI path resolution failed: ${JSON.stringify(e)}`);
        }
        if (resolved) {
          try {
            const stat: fs.Stat = fs.statSync(resolved);
            console.info(`[RapidRAW] pick_folder stat ok: isDirectory=${stat.isDirectory()}`);
          } catch (e) {
            console.error(`[RapidRAW] pick_folder stat FAILED for ${resolved}: ${JSON.stringify(e)}`);
          }
        }
      }
      resolvePickFolder(requestId, resolved);
    }).catch((e: BusinessError) => {
      console.error(`[RapidRAW] folder picker failed: ${JSON.stringify(e)}`);
      resolvePickFolder(requestId, null);
    });
  }

  private pickFiles(req: Record<string, Object>, requestId: number, resolvePickFiles: ESObject): void {
    const extensions: Array<string> = (req['extensions'] as Array<string>) ?? [];
    const maxSelect: number = (req['maxSelect'] as number) ?? 500;
    const options: picker.DocumentSelectOptions = new picker.DocumentSelectOptions();
    options.maxSelectNumber = Math.min(maxSelect, 500);
    if (extensions.length > 0) {
      // Suffix filters use the '.ext' form.
      options.fileSuffixFilters = extensions.map((ext: string): string =>
        ext.startsWith('.') ? ext.toLowerCase() : `.${ext.toLowerCase()}`);
      console.info(`[RapidRAW] pick_files filters: ${JSON.stringify(options.fileSuffixFilters)}`);
    }
    const documentPicker = new picker.DocumentViewPicker(this.context);
    documentPicker.select(options).then((uris: Array<string>) => {
      const paths: Array<string> = [];
      if (uris) {
        for (const rawUri of uris) {
          try {
            paths.push(new fileUri.FileUri(rawUri).path || rawUri);
          } catch (e) {
            console.error(`[RapidRAW] file URI path resolution failed for ${rawUri}: ${JSON.stringify(e)}`);
            paths.push(rawUri);
          }
        }
      }
      console.info(`[RapidRAW] pick_files -> ${paths.length} file(s)`);
      resolvePickFiles(requestId, paths);
    }).catch((e: BusinessError) => {
      console.error(`[RapidRAW] file picker failed: ${JSON.stringify(e)}`);
      resolvePickFiles(requestId, []);
    });
  }

  private saveToGallery(req: Record<string, Object>, requestId: number, resolveSaveToGallery: ESObject): void {
    const tempPath: string = req['tempPath'] as string;
    const fileName: string = req['fileName'] as string;
    const dotIndex: number = fileName.lastIndexOf('.');
    const extension: string = dotIndex >= 0 ? fileName.substring(dotIndex + 1).toLowerCase() : 'jpg';
    const title: string = dotIndex >= 0 ? fileName.substring(0, dotIndex) : fileName;
    console.info(`[RapidRAW] save_to_gallery tempPath: ${tempPath}`);
    // Tauri's desktop path resolver yields a non-sandbox cache path on OHOS,
    // which the system save dialog cannot authorize (14000011). Stage the
    // bytes into this ability's cacheDir first: URIs derived from it are the
    // canonical sandbox form showAssetsCreationDialog accepts.
    const sandboxDir: string = `${this.context.cacheDir}/export_bridge`;
    const stagedName: string = tempPath.substring(tempPath.lastIndexOf('/') + 1);
    const sandboxPath: string = `${sandboxDir}/${stagedName}`;
    try {
      // OHOS mkdirSync is not idempotent: an existing directory throws
      // 13900015 ("File exists") even with recursion enabled, so probe
      // for existence first.
      try {
        fs.accessSync(sandboxDir);
      } catch (accessError) {
        fs.mkdirSync(sandboxDir, true);
      }
      fs.copyFileSync(tempPath, sandboxPath);
    } catch (e) {
      console.error(`[RapidRAW] save_to_gallery staging failed: ${JSON.stringify(e)}`);
      resolveSaveToGallery(requestId, false, `staging failed: ${JSON.stringify(e)}`);
      return;
    }
    const srcUri: string = fileUri.getUriFromPath(sandboxPath);
    const helper: photoAccessHelper.PhotoAccessHelper =
      photoAccessHelper.getPhotoAccessHelper(this.context);
    const config: photoAccessHelper.PhotoCreationConfig = {
      title: title,
      fileNameExtension: extension,
      photoType: photoAccessHelper.PhotoType.IMAGE,
    };
    // createAsset needs the restricted WRITE_IMAGEVIDEO permission (fails
    // with error 201); this dialog saves with explicit user consent instead.
    // On consent the system creates the asset and returns a URI with a
    // permanent write grant, but the app must copy the bytes itself.
    helper.showAssetsCreationDialog([srcUri], [config]).then((uris: Array<string>) => {
      const first: string = uris.length > 0 ? uris[0] : '';
      if (first.startsWith('file://')) {
        try {
          const srcFile: fs.File = fs.openSync(sandboxPath, fs.OpenMode.READ_ONLY);
          const desFile: fs.File = fs.openSync(first, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE);
          fs.copyFileSync(srcFile.fd, desFile.fd);
          fs.closeSync(srcFile);
          fs.closeSync(desFile);
          console.info(`[RapidRAW] save_to_gallery ok: ${first}`);
          this.unlinkQuietly(sandboxPath);
          resolveSaveToGallery(requestId, true, null);
        } catch (e) {
          console.error(`[RapidRAW] save_to_gallery copy failed: ${JSON.stringify(e)}`);
          this.unlinkQuietly(sandboxPath);
          resolveSaveToGallery(requestId, false, `copy failed: ${JSON.stringify(e)}`);
        }
      } else if (first !== '') {
        // A non-URI entry is a documented batch-creation error code.
        console.error(`[RapidRAW] save_to_gallery failed: ${first}`);
        this.unlinkQuietly(sandboxPath);
        resolveSaveToGallery(requestId, false, `gallery save error ${first}`);
      } else {
        // An empty list means the user rejected the save dialog.
        console.info('[RapidRAW] save_to_gallery rejected by user');
        this.unlinkQuietly(sandboxPath);
        resolveSaveToGallery(requestId, false, 'user rejected the gallery save');
      }
    }).catch((e: BusinessError) => {
      console.error(`[RapidRAW] save_to_gallery dialog failed: ${JSON.stringify(e)}`);
      this.unlinkQuietly(sandboxPath);
      resolveSaveToGallery(requestId, false, `showAssetsCreationDialog failed: ${JSON.stringify(e)}`);
    });
  }

  private saveFileAs(req: Record<string, Object>, requestId: number, resolveSaveFileAs: ESObject): void {
    const fileName: string = req['fileName'] as string;
    const tempPath: string | null = req['tempPath'] === undefined ? null : req['tempPath'] as string;
    const options: picker.DocumentSaveOptions = new picker.DocumentSaveOptions();
    options.newFileNames = [fileName];
    const documentPicker = new picker.DocumentViewPicker(this.context);
    documentPicker.save(options).then((uris: Array<string>) => {
      if (!uris || uris.length === 0) {
        console.info('[RapidRAW] save_file_as cancelled');
        resolveSaveFileAs(requestId, null, null);
        return;
      }
      const targetPath: string = new fileUri.FileUri(uris[0]).path || uris[0];
      console.info(`[RapidRAW] save_file_as target: ${targetPath}`);
      if (tempPath !== null) {
        try {
          fs.copyFileSync(tempPath, targetPath);
          this.unlinkQuietly(tempPath);
        } catch (e) {
          console.error(`[RapidRAW] save_file_as copy failed: ${JSON.stringify(e)}`);
          resolveSaveFileAs(requestId, null, `copy failed: ${JSON.stringify(e)}`);
          return;
        }
      }
      resolveSaveFileAs(requestId, targetPath, null);
    }).catch((e: BusinessError) => {
      console.error(`[RapidRAW] save picker failed: ${JSON.stringify(e)}`);
      resolveSaveFileAs(requestId, null, JSON.stringify(e));
    });
  }

  private unlinkQuietly(path: string): void {
    try {
      fs.unlinkSync(path);
    } catch (e) {
      // The Rust side removes the temp file as a backstop.
      console.info(`[RapidRAW] temp unlink skipped: ${JSON.stringify(e)}`);
    }
  }
}
