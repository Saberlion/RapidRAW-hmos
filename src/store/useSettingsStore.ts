import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { platform } from '@tauri-apps/plugin-os';
import { AppSettings, SupportedTypes, Invokes } from '../components/ui/AppProperties';
import { DEFAULT_THEME_ID } from '../utils/themes';

interface SettingsState {
  appSettings: AppSettings | null;
  theme: string;
  supportedTypes: SupportedTypes | null;
  osPlatform: string;
  isOhos: boolean;
  isSystemDark: boolean | null;

  // Actions
  initPlatform: () => void;
  setAppSettings: (settings: AppSettings | null) => void;
  setTheme: (theme: string) => void;
  setSupportedTypes: (types: SupportedTypes | null) => void;
  setSystemDark: (dark: boolean) => void;
  handleSettingsChange: (newSettings: AppSettings) => Promise<void>;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  appSettings: null,
  theme: DEFAULT_THEME_ID,
  supportedTypes: null,
  osPlatform: '',
  isOhos: false,
  isSystemDark: null,

  initPlatform: () => {
    try {
      set({ osPlatform: platform() });
    } catch (_err) {
      set({ osPlatform: '' });
    }
    // plugin-os reports "linux" on OpenHarmony; detect OHOS through the Rust
    // bridge instead (same pattern as the TitleBar window controls).
    invoke<boolean>(Invokes.IsOhosBuild)
      .then((ohos) => set({ isOhos: ohos }))
      .catch(() => set({ isOhos: false }));
  },

  setAppSettings: (settings) => set({ appSettings: settings }),

  setTheme: (theme) => set({ theme }),

  setSupportedTypes: (types) => set({ supportedTypes: types }),

  setSystemDark: (dark) => set({ isSystemDark: dark }),

  handleSettingsChange: async (newSettings: AppSettings) => {
    if (!newSettings) {
      console.error('handleSettingsChange was called with null settings. Aborting save operation.');
      return;
    }

    if (newSettings.theme && newSettings.theme !== get().theme) {
      set({ theme: newSettings.theme });
    }

    const { searchCriteria: _searchCriteria, ...settingsToSave } = newSettings as any;
    set({ appSettings: newSettings });

    try {
      await invoke(Invokes.SaveSettings, { settings: settingsToSave });
    } catch (err) {
      console.error('Failed to save settings:', err);
    }
  },
}));
