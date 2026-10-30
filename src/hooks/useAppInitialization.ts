import { useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { locale } from '@tauri-apps/plugin-os';
import { useShallow } from 'zustand/react/shallow';
import { useSettingsStore } from '../store/useSettingsStore';
import { useUIStore, reconcileWorkspace } from '../store/useUIStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useEditorStore } from '../store/useEditorStore';
import { useProcessStore } from '../store/useProcessStore';
import { THEMES, DEFAULT_THEME_ID, ThemeProps, resolveThemeId } from '../utils/themes';
import { fetchOhosPrivacyState } from '../utils/ohosPrivacy';
import { COPYABLE_ADJUSTMENT_KEYS, withAdjustmentLayout } from '../utils/adjustments';
import {
  FALLBACK_LANGUAGE,
  detectSystemLanguage,
  resolveLanguageTag,
  type SupportedLanguage,
} from '../i18n';
import {
  FilterCriteria,
  Invokes,
  LibraryViewMode,
  RawStatus,
  EditedStatus,
  FlagStatus,
  Theme,
  ThumbnailSize,
  ThumbnailAspectRatio,
} from '../components/ui/AppProperties';
import { useTranslation } from 'react-i18next';

interface UseAppInitializationProps {
  preloadedDataRef: React.RefObject<any>;
  thumbnailSize: ThumbnailSize;
  setThumbnailSize: (size: ThumbnailSize) => void;
  thumbnailAspectRatio: ThumbnailAspectRatio;
  setThumbnailAspectRatio: (ratio: ThumbnailAspectRatio) => void;
  libraryViewMode: LibraryViewMode;
  setLibraryViewMode: (mode: LibraryViewMode) => void;
}

/**
 * Read the system locale once (authoritative source: plugin-os, which reports the
 * real OS locale on OHOS). `navigator` stays as a fallback for builds where the
 * plugin or its permission is unavailable.
 */
const readSystemLanguage = async (): Promise<SupportedLanguage> => {
  // The webview locale follows the OS locale on OHOS; plugin-os is the fallback
  // (it reports the wrong value on some firmware builds).
  const fromNavigator = detectSystemLanguage();
  if (fromNavigator !== FALLBACK_LANGUAGE) return fromNavigator;
  try {
    const resolved = resolveLanguageTag(await locale());
    if (resolved) return resolved;
  } catch {
    // Keep the navigator fallback.
  }
  return FALLBACK_LANGUAGE;
};

export const useAppInitialization = ({
  preloadedDataRef,
  thumbnailSize,
  setThumbnailSize,
  thumbnailAspectRatio,
  setThumbnailAspectRatio,
  libraryViewMode,
  setLibraryViewMode,
}: UseAppInitializationProps) => {
  const isInitialMount = useRef(true);
  const { i18n } = useTranslation();

  const {
    appSettings,
    theme,
    osPlatform,
    isOhos,
    isSystemDark,
    setAppSettings,
    setTheme,
    setSupportedTypes,
    initPlatform,
    handleSettingsChange,
  } = useSettingsStore(
    useShallow((state) => ({
      appSettings: state.appSettings,
      theme: state.theme,
      osPlatform: state.osPlatform,
      isOhos: state.isOhos,
      isSystemDark: state.isSystemDark,
      setAppSettings: state.setAppSettings,
      setTheme: state.setTheme,
      setSupportedTypes: state.setSupportedTypes,
      initPlatform: state.initPlatform,
      handleSettingsChange: state.handleSettingsChange,
    })),
  );

  const { uiVisibility, collapsibleSectionsState, setUI } = useUIStore(
    useShallow((state) => ({
      uiVisibility: state.uiVisibility,
      collapsibleSectionsState: state.collapsibleSectionsState,
      setUI: state.setUI,
    })),
  );

  const workspaceProps = useUIStore(
    useShallow((state) => ({
      leftPanelWidth: state.leftPanelWidth,
      rightPanelWidth: state.rightPanelWidth,
      leftTopHeight: state.leftTopHeight,
      rightTopHeight: state.rightTopHeight,
      panelLayout: state.panelLayout,
      activePanels: state.activePanels,
      panelSwitcherPlacement: state.panelSwitcherPlacement,
    })),
  );

  const {
    sortCriteria,
    filterCriteria,
    currentFolderPath,
    expandedFolders,
    activeAlbumId,
    expandedAlbumGroups,
    setSortCriteria,
    setFilterCriteria,
    setLibrary,
  } = useLibraryStore(
    useShallow((state) => ({
      sortCriteria: state.sortCriteria,
      filterCriteria: state.filterCriteria,
      currentFolderPath: state.currentFolderPath,
      expandedFolders: state.expandedFolders,
      activeAlbumId: state.activeAlbumId,
      expandedAlbumGroups: state.expandedAlbumGroups,
      setSortCriteria: state.setSortCriteria,
      setFilterCriteria: state.setFilterCriteria,
      setLibrary: state.setLibrary,
    })),
  );

  const { setEditor } = useEditorStore(
    useShallow((state) => ({
      setEditor: state.setEditor,
    })),
  );

  const isAndroid = osPlatform === 'android';
  const defaultThumbnailSize = isAndroid ? ThumbnailSize.Small : ThumbnailSize.Medium;
  const defaultLibraryViewMode = isAndroid ? LibraryViewMode.Recursive : LibraryViewMode.Flat;
  const prevImageCountsNeed = useRef<boolean | undefined>(undefined);

  useEffect(() => {
    initPlatform();
  }, [initPlatform]);

  // Source of truth for Theme.System: the OHOS bridge (initial query + event,
  // authoritative) or the standard media query everywhere else.
  useEffect(() => {
    let active = true;
    const unlisteners: Array<() => void> = [];

    invoke<boolean | null>(Invokes.GetOhosColorMode)
      .then((dark) => {
        if (active && typeof dark === 'boolean') useSettingsStore.getState().setSystemDark(dark);
      })
      .catch(() => undefined);

    listen<{ dark: boolean }>('ohos-color-mode', (event) => {
      useSettingsStore.getState().setSystemDark(event.payload.dark);
    })
      .then((unlisten) => {
        if (active) unlisteners.push(unlisten);
        else unlisten();
      })
      .catch(() => undefined);

    // AppGallery privacy management service: the store owns first-launch
    // consent, so the app only mirrors it here (links + verdict) and re-reads it
    // whenever the store reports a change.
    if (isOhos) {
      fetchOhosPrivacyState()
        .then((state) => {
          if (active) useSettingsStore.getState().setOhosPrivacy(state);
        })
        .catch(() => undefined);

      listen('ohos-privacy-state', () => {
        fetchOhosPrivacyState()
          .then((state) => useSettingsStore.getState().setOhosPrivacy(state))
          .catch(() => undefined);
      })
        .then((unlisten) => {
          if (active) unlisteners.push(unlisten);
          else unlisten();
        })
        .catch(() => undefined);
    }

    const media = window.matchMedia('(prefers-color-scheme: dark)');
    useSettingsStore.getState().setSystemDark(media.matches);
    const onMediaChange = (event: MediaQueryListEvent) => useSettingsStore.getState().setSystemDark(event.matches);
    media.addEventListener('change', onMediaChange);

    return () => {
      active = false;
      unlisteners.forEach((unlisten) => unlisten());
      media.removeEventListener('change', onMediaChange);
    };
  }, [isOhos]);

  useEffect(() => {
    invoke(Invokes.GetSupportedFileTypes)
      .then((types: any) => setSupportedTypes(types))
      .catch((err) => console.error('Failed to load supported file types:', err));
  }, [setSupportedTypes]);

  useEffect(() => {
    Promise.all([invoke(Invokes.LoadSettings), invoke<boolean>(Invokes.IsTetheringSupported).catch(() => false)])
      .then(async ([settings, isTetheringSupported]: [any, boolean]) => {
        if (
          !settings.copyPasteSettings ||
          !settings.copyPasteSettings.includedAdjustments ||
          settings.copyPasteSettings.includedAdjustments.length === 0
        ) {
          settings.copyPasteSettings = { mode: 'merge', includedAdjustments: COPYABLE_ADJUSTMENT_KEYS };
        }

        if (!settings.language) {
          // First run: read the OS locale once and persist it as the default.
          settings.language = await readSystemLanguage();
          handleSettingsChange(settings);
        } else {
          // Normalize legacy/invalid saved tags (e.g. `zh`, `en-US`) to a shipped id.
          const normalized = resolveLanguageTag(settings.language);
          if (normalized && normalized !== settings.language) {
            settings.language = normalized;
            handleSettingsChange(settings);
          }
        }

        const savedRawStatus = settings?.filterCriteria?.rawStatus as string | undefined;
        if (savedRawStatus === 'groupVariants' || savedRawStatus === 'rawOverNonRaw') {
          const legacyPref = settings?.groupPreferredType === 'jpeg' ? 'jpeg' : 'raw';
          settings.grouping = legacyPref;
          settings.filterCriteria = { ...settings.filterCriteria, rawStatus: 'all' };
          handleSettingsChange(settings);
        }

        const reconciledWorkspace = reconcileWorkspace(settings?.workspace, isTetheringSupported);
        settings.workspace = reconciledWorkspace;

        setAppSettings(settings);
        i18n.changeLanguage(settings.language);

        if (settings?.sortCriteria) setSortCriteria(settings.sortCriteria);

        if (settings?.filterCriteria) {
          setFilterCriteria((prev: FilterCriteria) => ({
            ...prev,
            ...settings.filterCriteria,
            rawStatus: settings.filterCriteria.rawStatus || RawStatus.All,
            editedStatus: settings.filterCriteria.editedStatus || EditedStatus.All,
            flagStatus: settings.filterCriteria.flagStatus || FlagStatus.All,
            colors: settings.filterCriteria.colors || [],
          }));
        }

        if (settings?.theme) setTheme(settings.theme);

        if (settings?.uiVisibility) {
          setUI((state) => ({ uiVisibility: { ...state.uiVisibility, ...settings.uiVisibility } }));
        }

        if (settings?.adjustmentLayout?.openSections) {
          setUI((state) => ({
            collapsibleSectionsState: { ...state.collapsibleSectionsState, ...settings.adjustmentLayout.openSections },
          }));
        }

        setUI({
          leftPanelWidth: reconciledWorkspace.leftPanelWidth,
          rightPanelWidth: reconciledWorkspace.rightPanelWidth,
          leftTopHeight: reconciledWorkspace.leftTopHeight,
          rightTopHeight: reconciledWorkspace.rightTopHeight,
          panelLayout: reconciledWorkspace.panelLayout,
          activePanels: reconciledWorkspace.activePanels,
          panelSwitcherPlacement: reconciledWorkspace.panelSwitcherPlacement,
        });

        if (settings?.isWaveformVisible !== undefined) setEditor({ isWaveformVisible: settings.isWaveformVisible });
        if (settings?.activeWaveformChannel) setEditor({ activeWaveformChannel: settings.activeWaveformChannel });
        if (typeof settings?.waveformHeight === 'number') setEditor({ waveformHeight: settings.waveformHeight });

        setLibraryViewMode(settings?.libraryViewMode ?? defaultLibraryViewMode);
        setThumbnailSize(settings?.thumbnailSize ?? defaultThumbnailSize);
        if (settings?.thumbnailAspectRatio) setThumbnailAspectRatio(settings.thumbnailAspectRatio);

        if (settings?.pinnedFolders && settings.pinnedFolders.length > 0) {
          try {
            const trees = await invoke(Invokes.GetPinnedFolderTrees, {
              paths: settings.pinnedFolders,
              expandedFolders: settings.lastFolderState?.expandedFolders || [],
              showImageCounts: settings.enableFolderImageCounts || settings.folderTreeSort?.key === 'imageCount',
            });
            setLibrary({ pinnedFolderTrees: trees });
          } catch (err) {
            console.error('Failed to load pinned folder trees:', err);
          }
        }

        const rootFolders = settings.rootFolders?.length
          ? settings.rootFolders
          : settings.lastRootPath
            ? [settings.lastRootPath]
            : [];

        if (!isAndroid && rootFolders.length > 0) {
          const currentPath = settings.lastFolderState?.currentFolderPath || rootFolders[0];
          const isAlbum = currentPath.startsWith('Album: ');
          const command =
            settings.libraryViewMode === LibraryViewMode.Recursive
              ? Invokes.ListImagesRecursive
              : Invokes.ListImagesInDir;

          preloadedDataRef.current = {
            rootPaths: rootFolders,
            currentPath: currentPath,
            trees: invoke(Invokes.GetPinnedFolderTrees, {
              paths: rootFolders,
              expandedFolders: settings.lastFolderState?.expandedFolders ?? rootFolders,
              showImageCounts: settings.enableFolderImageCounts || settings.folderTreeSort?.key === 'imageCount',
            }),
            // A dead root (revoked FileKit grant) rejects this promise long
            // before anyone consumes it — swallow it here so it does not
            // surface as an unhandled rejection at startup; consumers treat
            // undefined as "no preload" and load fresh.
            images: isAlbum ? undefined : invoke(command, { path: currentPath }).catch(() => undefined),
          };
        }

        if (settings?.lastFolderState) {
          setLibrary({
            expandedFolders: new Set(settings.lastFolderState.expandedFolders || []),
            expandedAlbumGroups: new Set(settings.lastFolderState.expandedAlbumGroups || []),
          });
        }

        invoke('frontend_ready')
          .then((launch: any) => {
            if (launch?.editSession) {
              useProcessStore.getState().setProcess({ externalEditSession: launch.editSession });
            } else if (launch?.openWithFile) {
              useProcessStore.getState().setProcess({ initialFileToOpen: launch.openWithFile });
            }
          })
          .catch((e) => console.error('Failed to notify backend of readiness:', e));
      })
      .catch((err) => {
        console.error('Failed to load settings:', err);
        setAppSettings({
          lastRootPath: null,
          theme: DEFAULT_THEME_ID as Theme,
          thumbnailSize: defaultThumbnailSize,
          libraryViewMode: defaultLibraryViewMode,
        });
      })
      .finally(() => {
        isInitialMount.current = false;
      });
  }, [
    isAndroid,
    setAppSettings,
    setTheme,
    setUI,
    defaultLibraryViewMode,
    defaultThumbnailSize,
    setSortCriteria,
    setFilterCriteria,
    setEditor,
    setLibrary,
    preloadedDataRef,
    setLibraryViewMode,
    setThumbnailSize,
    setThumbnailAspectRatio,
  ]);

  useEffect(() => {
    if (isInitialMount.current || !appSettings) return;

    const currentWorkspaceStr = JSON.stringify(appSettings.workspace || {});
    const newWorkspaceStr = JSON.stringify(workspaceProps);

    if (currentWorkspaceStr !== newWorkspaceStr) {
      const timeoutId = setTimeout(() => {
        handleSettingsChange({ ...appSettings, workspace: workspaceProps });
      }, 500);

      return () => clearTimeout(timeoutId);
    }
  }, [workspaceProps, appSettings, handleSettingsChange]);

  useEffect(() => {
    if (isInitialMount.current || !appSettings) return;
    if (JSON.stringify(appSettings.uiVisibility) !== JSON.stringify(uiVisibility)) {
      handleSettingsChange({ ...appSettings, uiVisibility });
    }
  }, [uiVisibility, appSettings, handleSettingsChange]);

  useEffect(() => {
    if (isInitialMount.current || !appSettings) return;
    if (JSON.stringify(appSettings.adjustmentLayout?.openSections) !== JSON.stringify(collapsibleSectionsState)) {
      handleSettingsChange(withAdjustmentLayout(appSettings, { openSections: collapsibleSectionsState }));
    }
  }, [collapsibleSectionsState, appSettings, handleSettingsChange]);

  useEffect(() => {
    if (isInitialMount.current || !appSettings) return;
    if (appSettings.thumbnailSize !== thumbnailSize) {
      handleSettingsChange({ ...appSettings, thumbnailSize });
    }
  }, [thumbnailSize, appSettings, handleSettingsChange]);

  useEffect(() => {
    if (isInitialMount.current || !appSettings) return;
    if (appSettings.thumbnailAspectRatio !== thumbnailAspectRatio) {
      handleSettingsChange({ ...appSettings, thumbnailAspectRatio });
    }
  }, [thumbnailAspectRatio, appSettings, handleSettingsChange]);

  useEffect(() => {
    if (isInitialMount.current || !appSettings) return;
    if (appSettings.libraryViewMode !== libraryViewMode) {
      handleSettingsChange({ ...appSettings, libraryViewMode });
    }
  }, [libraryViewMode, appSettings, handleSettingsChange]);

  useEffect(() => {
    if (isInitialMount.current || !appSettings) return;
    if (JSON.stringify(appSettings.sortCriteria) !== JSON.stringify(sortCriteria)) {
      handleSettingsChange({ ...appSettings, sortCriteria });
    }
  }, [sortCriteria, appSettings, handleSettingsChange]);

  useEffect(() => {
    if (isInitialMount.current || !appSettings) return;
    if (JSON.stringify(appSettings.filterCriteria) !== JSON.stringify(filterCriteria)) {
      handleSettingsChange({ ...appSettings, filterCriteria });
    }
  }, [filterCriteria, appSettings, handleSettingsChange]);

  useEffect(() => {
    if (isInitialMount.current || !appSettings) return;
    if (appSettings.language && appSettings.language !== i18n.language) {
      i18n.changeLanguage(appSettings.language);
    }
  }, [appSettings?.language, i18n.language]);

  useEffect(() => {
    if (isInitialMount.current || !appSettings) return;
    if (!currentFolderPath && !activeAlbumId) return;

    const currentExpanded = Array.from(expandedFolders);
    const currentExpandedAlbums = Array.from(expandedAlbumGroups);

    const prevFolderState = appSettings.lastFolderState || {
      currentFolderPath: null,
      expandedFolders: [],
      activeAlbumId: null,
      expandedAlbumGroups: [],
    };

    const pathChanged = prevFolderState.currentFolderPath !== currentFolderPath;
    const expandedChanged = JSON.stringify(prevFolderState.expandedFolders || []) !== JSON.stringify(currentExpanded);
    const albumChanged = prevFolderState.activeAlbumId !== activeAlbumId;
    const albumExpandedChanged =
      JSON.stringify(prevFolderState.expandedAlbumGroups || []) !== JSON.stringify(currentExpandedAlbums);

    if (pathChanged || expandedChanged || albumChanged || albumExpandedChanged) {
      handleSettingsChange({
        ...appSettings,
        lastFolderState: {
          currentFolderPath,
          expandedFolders: currentExpanded,
          activeAlbumId,
          expandedAlbumGroups: currentExpandedAlbums,
        },
      });
    }
  }, [currentFolderPath, expandedFolders, activeAlbumId, expandedAlbumGroups, appSettings, handleSettingsChange]);

  useEffect(() => {
    if (!appSettings) return;

    const needsImageCounts = Boolean(
      appSettings.enableFolderImageCounts || appSettings.folderTreeSort?.key === 'imageCount',
    );

    if (prevImageCountsNeed.current === undefined) {
      prevImageCountsNeed.current = needsImageCounts;
      return;
    }

    if (prevImageCountsNeed.current !== needsImageCounts) {
      prevImageCountsNeed.current = needsImageCounts;

      const rootFolders = appSettings.rootFolders?.length
        ? appSettings.rootFolders
        : appSettings.lastRootPath
          ? [appSettings.lastRootPath]
          : [];
      const pinnedFolders = appSettings.pinnedFolders || [];

      const currentExpanded = Array.from(useLibraryStore.getState().expandedFolders);

      setLibrary({ isTreeLoading: true });

      const promises = [];

      if (pinnedFolders.length > 0) {
        promises.push(
          invoke(Invokes.GetPinnedFolderTrees, {
            paths: pinnedFolders,
            expandedFolders: currentExpanded,
            showImageCounts: needsImageCounts,
          }).then((trees: any) => ({ type: 'pinned', trees })),
        );
      }

      if (rootFolders.length > 0) {
        promises.push(
          invoke(Invokes.GetPinnedFolderTrees, {
            paths: rootFolders,
            expandedFolders: currentExpanded,
            showImageCounts: needsImageCounts,
          }).then((trees: any) => ({ type: 'root', trees })),
        );
      }

      Promise.all(promises)
        .then((results) => {
          useLibraryStore.getState().setLibrary((_state) => {
            const updates: any = { isTreeLoading: false };
            results.forEach((res) => {
              if (res.type === 'pinned') updates.pinnedFolderTrees = res.trees;
              if (res.type === 'root') updates.folderTrees = res.trees;
            });
            return updates;
          });
        })
        .catch((err) => {
          console.error('Failed to re-fetch trees for image counts:', err);
          setLibrary({ isTreeLoading: false });
        });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [appSettings?.enableFolderImageCounts, appSettings?.folderTreeSort?.key]);

  useEffect(() => {
    const root = document.documentElement;
    const resolvedThemeId = resolveThemeId(theme || DEFAULT_THEME_ID, isSystemDark);

    const baseTheme =
      THEMES.find((t: ThemeProps) => t.id === resolvedThemeId) ||
      THEMES.find((t: ThemeProps) => t.id === DEFAULT_THEME_ID);
    if (!baseTheme) return;

    const finalCssVariables: any = { ...baseTheme.cssVariables };

    Object.entries(finalCssVariables).forEach(([key, value]) => {
      root.style.setProperty(key, value as string);
    });

    const fontFamily = appSettings?.fontFamily || 'poppins';
    const fontStack =
      fontFamily === 'system'
        ? '-apple-system, BlinkMacSystemFont, system-ui, sans-serif'
        : "'Poppins', system-ui, sans-serif";
    root.style.setProperty('--font-family', fontStack);
  }, [theme, isSystemDark, appSettings?.fontFamily]);
};
