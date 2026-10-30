import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';

import en from './locales/en.json';
import de from './locales/de.json';
import zhCN from './locales/zh-CN.json';
import zhTW from './locales/zh-TW.json';
import pl from './locales/pl.json';
import es from './locales/es.json';
import fr from './locales/fr.json';
import it from './locales/it.json';
import pt from './locales/pt.json';
import ja from './locales/ja.json';
import ko from './locales/ko.json';
import ru from './locales/ru.json';
import ca from './locales/ca.json';
import nl from './locales/nl.json';
import cs from './locales/cs.json';

/** Locale ids shipped in `src/i18n/locales/` and offered in Settings. */
export const SUPPORTED_LANGUAGES = [
  'en',
  'ca',
  'de',
  'es',
  'fr',
  'it',
  'nl',
  'pl',
  'pt',
  'ru',
  'ja',
  'ko',
  'cs',
  'zh-CN',
  'zh-TW',
] as const;

export type SupportedLanguage = (typeof SUPPORTED_LANGUAGES)[number];

export const FALLBACK_LANGUAGE: SupportedLanguage = 'en';

/**
 * Map an OS locale / BCP-47 tag (`zh-Hans-CN`, `de-AT`, `pt_BR`) onto one of the
 * shipped locale ids. Chinese needs explicit script/region mapping because both
 * `zh-CN` and `zh-TW` use the same primary subtag.
 */
export function resolveLanguageTag(tag: string | null | undefined): SupportedLanguage | null {
  if (!tag) return null;
  const normalized = tag.trim().replace(/_/g, '-').toLowerCase();
  if (!normalized) return null;

  // Chinese: Traditional for Hant/TW/HK/MO, Simplified for everything else.
  if (normalized === 'zh' || normalized.startsWith('zh-')) {
    return /hant|tw|hk|mo/.test(normalized) ? 'zh-TW' : 'zh-CN';
  }

  const exact = SUPPORTED_LANGUAGES.find((language) => language.toLowerCase() === normalized);
  if (exact) return exact;

  // Region variants of single-id locales: `de-AT` -> `de`, `pt-BR` -> `pt`.
  const primary = normalized.split('-')[0];
  return SUPPORTED_LANGUAGES.find((language) => language === primary) ?? null;
}

/**
 * Best-effort synchronous system language read (navigator). Used for the very
 * first paint; the authoritative OS locale is read once at startup via
 * `@tauri-apps/plugin-os`.
 */
export function detectSystemLanguage(): SupportedLanguage {
  if (typeof navigator === 'undefined') return FALLBACK_LANGUAGE;
  const candidates = [navigator.language, ...(navigator.languages ?? [])];
  for (const candidate of candidates) {
    const resolved = resolveLanguageTag(candidate);
    if (resolved) return resolved;
  }
  return FALLBACK_LANGUAGE;
}

i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
    de: { translation: de },
    'zh-CN': { translation: zhCN },
    'zh-TW': { translation: zhTW },
    pl: { translation: pl },
    es: { translation: es },
    fr: { translation: fr },
    it: { translation: it },
    pt: { translation: pt },
    ja: { translation: ja },
    ko: { translation: ko },
    ru: { translation: ru },
    ca: { translation: ca },
    nl: { translation: nl },
    cs: { translation: cs },
  },
  lng: detectSystemLanguage(),
  supportedLngs: [...SUPPORTED_LANGUAGES],
  fallbackLng: FALLBACK_LANGUAGE,
  returnEmptyString: false,
  interpolation: {
    escapeValue: false,
  },
});
