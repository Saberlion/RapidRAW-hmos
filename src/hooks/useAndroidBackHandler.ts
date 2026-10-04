import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useUIStore } from '../store/useUIStore';
import { useSettingsStore } from '../store/useSettingsStore';

export function useAndroidBackHandler() {
  const isOhos = useSettingsStore((state) => state.isOhos);

  useEffect(() => {
    const osPlatform = useSettingsStore.getState().osPlatform;
    const isAndroid = osPlatform === 'android';
    // plugin-os reports "linux" on OHOS, so the Android guard alone would
    // never install the handler there: the OHOS back gesture arrives as the
    // `ohos-back-pressed` tauri event instead (MainPage → on_back_press_intercept).
    if (!isAndroid && !isOhos) return;

    const handleBack = () => {
      const ui = useUIStore.getState();

      if (ui.confirmModalState.isOpen) {
        ui.setUI((state: any) => ({ confirmModalState: { ...state.confirmModalState, isOpen: false } }));
        return;
      }
      if (ui.isCreateFolderModalOpen) {
        ui.setUI({ isCreateFolderModalOpen: false });
        return;
      }
      if (ui.isRenameFolderModalOpen) {
        ui.setUI({ isRenameFolderModalOpen: false });
        return;
      }
      if (ui.isRenameFileModalOpen) {
        ui.setUI({ isRenameFileModalOpen: false });
        return;
      }
      if (ui.isImportModalOpen) {
        ui.setUI({ isImportModalOpen: false });
        return;
      }
      if (ui.isCopyPasteSettingsModalOpen) {
        ui.setUI({ isCopyPasteSettingsModalOpen: false });
        return;
      }
      if (ui.isCreateAlbumModalOpen) {
        ui.setUI({ isCreateAlbumModalOpen: false });
        return;
      }
      if (ui.isCreateAlbumGroupModalOpen) {
        ui.setUI({ isCreateAlbumGroupModalOpen: false });
        return;
      }
      if (ui.isRenameAlbumModalOpen) {
        ui.setUI({ isRenameAlbumModalOpen: false });
        return;
      }
      if (ui.panoramaModalState.isOpen) {
        ui.setUI({
          panoramaModalState: {
            isOpen: false,
            isProcessing: false,
            progressMessage: '',
            finalImageBase64: null,
            error: null,
            stitchingSourcePaths: [],
          },
        });
        return;
      }
      if (ui.hdrModalState.isOpen) {
        ui.setUI({
          hdrModalState: {
            isOpen: false,
            isProcessing: false,
            progressMessage: '',
            finalImageBase64: null,
            error: null,
            stitchingSourcePaths: [],
          },
        });
        return;
      }
      if (ui.negativeModalState.isOpen) {
        ui.setUI((state: any) => ({ negativeModalState: { ...state.negativeModalState, isOpen: false } }));
        return;
      }
      if (ui.denoiseModalState.isOpen) {
        ui.setUI((state: any) => ({ denoiseModalState: { ...state.denoiseModalState, isOpen: false } }));
        return;
      }
      if (ui.cullingModalState.isOpen) {
        ui.setUI({
          cullingModalState: { isOpen: false, progress: null, suggestions: null, error: null, pathsToCull: [] },
        });
        return;
      }
      if (ui.collageModalState.isOpen) {
        ui.setUI({ collageModalState: { isOpen: false, sourceImages: [] } });
        return;
      }

      window.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true, cancelable: true }),
      );
    };

    (window as any).__handleAndroidBack = handleBack;

    let active = true;
    let unlisten: (() => void) | undefined;
    if (isOhos) {
      listen('ohos-back-pressed', () => handleBack())
        .then((un) => {
          if (active) unlisten = un;
          else un();
        })
        .catch(() => undefined);
    }

    return () => {
      active = false;
      unlisten?.();
      delete (window as any).__handleAndroidBack;
    };
  }, [isOhos]);
}
