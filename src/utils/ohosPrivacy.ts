import { invoke } from '@tauri-apps/api/core';
import { Invokes } from '../components/ui/AppProperties';

/**
 * OpenHarmony AppGallery privacy management service (`privacyManager` from
 * `@kit.AppGalleryKit`) access layer.
 *
 * When the app uses AppGallery's standardized privacy-statement hosting
 * service, the store owns first-launch consent and shows its own dialog; the
 * app must not draw a second one (that is why App.tsx skips AgreementGate on
 * OHOS — docs 6.17). This module drives the active half of the integration:
 * querying the store-issued links and the consent state, re-requesting consent
 * and revoking it.
 *
 * The service is unavailable on emulators and in builds without the hosted
 * statement (AppGallery error 1006700003). Every call therefore resolves to a
 * payload rather than throwing, and the UI renders an "unknown" state.
 */

/** AppPrivacyLink: one store-issued privacy statement / agreement link. */
export interface OhosPrivacyLink {
  type?: number;
  versionCode?: number;
  url?: string;
}

/** AppPrivacyResult: the recorded consent verdict for one document. */
export interface OhosPrivacyResult {
  type?: number;
  versionCode?: number;
  /** 1 = accepted (full mode), 0 = not accepted. */
  result?: number;
  signingTimestamp?: number;
}

export interface OhosPrivacyState {
  ok: boolean;
  links?: OhosPrivacyLink[];
  results?: OhosPrivacyResult[];
  /** Present on `requestAppPrivacyConsent` results. */
  isAgreed?: boolean;
  error?: string | null;
  /** AppGallery error code when the call failed. */
  code?: number | null;
}

export type OhosConsent = 'granted' | 'denied' | 'unknown';

/** AppGallery error code: the app does not use privacy statement hosting. */
export const PRIVACY_NOT_INTEGRATED = 1006700003;

/**
 * This app's AGC-hosted privacy statement (the "standardized privacy statement
 * hosting" document published in AppGallery Connect). Used as the fallback
 * target when the privacy management service reports no link — i.e. hosting is
 * not configured yet, or the build has no `appgallery_privacy_*` metadata — so
 * the statement stays reachable from the About section.
 */
export const PRIVACY_STATEMENT_AGREEMENT_ID = '2068880948149116608';

const PRIVACY_STATEMENT_BASE = 'https://agreement-drcn.hispace.dbankcloud.cn/index.html';

/**
 * Build the hosted-statement URL for a UI language. The hosted page switches
 * content by its `lang` query parameter; only `zh` and `en` are published, so
 * every non-Chinese locale falls back to `en`.
 */
export function privacyStatementUrl(language: string): string {
  const lang = language.toLowerCase().startsWith('zh') ? 'zh' : 'en';
  return `${PRIVACY_STATEMENT_BASE}?lang=${lang}&agreementId=${PRIVACY_STATEMENT_AGREEMENT_ID}`;
}

const EMPTY: OhosPrivacyState = { ok: false, links: [], results: [], error: null, code: null };

function normalize(value: unknown): OhosPrivacyState {
  if (!value || typeof value !== 'object') return { ...EMPTY };
  const raw = value as Record<string, unknown>;
  return {
    ok: raw.ok === true,
    links: Array.isArray(raw.links) ? (raw.links as OhosPrivacyLink[]) : [],
    results: Array.isArray(raw.results) ? (raw.results as OhosPrivacyResult[]) : [],
    isAgreed: typeof raw.isAgreed === 'boolean' ? raw.isAgreed : undefined,
    error: typeof raw.error === 'string' ? raw.error : null,
    code: typeof raw.code === 'number' ? raw.code : null,
  };
}

function normalizeFailure(prefix: string, err: unknown): OhosPrivacyState {
  const message = err instanceof Error ? err.message : String(err);
  return { ...EMPTY, error: `${prefix}: ${message}` };
}

/** Query the store-issued links and the current consent state. */
export async function fetchOhosPrivacyState(): Promise<OhosPrivacyState> {
  try {
    return normalize(await invoke(Invokes.GetOhosPrivacyState));
  } catch (err) {
    return normalizeFailure('get_ohos_privacy_state failed', err);
  }
}

/** Raise AppGallery's standardized privacy dialog and return its result. */
export async function requestOhosPrivacyConsent(): Promise<OhosPrivacyState> {
  try {
    return normalize(await invoke(Invokes.RequestOhosPrivacyConsent));
  } catch (err) {
    return normalizeFailure('request_ohos_privacy_consent failed', err);
  }
}

/** Revoke the recorded consent (the next launch shows the dialog again). */
export async function revokeOhosPrivacyConsent(): Promise<OhosPrivacyState> {
  try {
    return normalize(await invoke(Invokes.RevokeOhosPrivacyConsent));
  } catch (err) {
    return normalizeFailure('revoke_ohos_privacy_consent failed', err);
  }
}

/**
 * Collapse a payload into a tri-state verdict. `unknown` means "the hosted
 * service is not in play" (unlisted build, emulator, or no signed document) —
 * callers must treat it as "do not gate anything", because the platform itself
 * refuses permission grants while the statement is unsigned.
 */
export function ohosConsentOf(state: OhosPrivacyState | null): OhosConsent {
  if (!state || !state.ok) return 'unknown';
  if (typeof state.isAgreed === 'boolean') return state.isAgreed ? 'granted' : 'denied';
  const results = state.results ?? [];
  if (results.length > 0) {
    return results.every((result) => result.result === 1) ? 'granted' : 'denied';
  }
  // No per-document verdict at all. When the platform is serving links, the
  // hosted statement IS in play, so "no result" means it has not been signed
  // yet — a freshly installed build reports exactly this (links present,
  // results []), and it must not read as the softer "unknown". After a revoke
  // the platform reports an explicit result: 0 instead.
  return (state.links ?? []).length > 0 ? 'denied' : 'unknown';
}

/** True when the service reported that the app does not use hosting. */
export function isPrivacyHostingMissing(state: OhosPrivacyState | null): boolean {
  return state?.code === PRIVACY_NOT_INTEGRATED;
}

export interface OhosPrivacyLinkEntry {
  /** i18n key for the row label (literal union so `t()` stays typed). */
  labelKey: 'settings.about.privacyPolicy' | 'settings.about.userAgreement';
  url: string;
}

/**
 * Turn the store-issued links into labelled rows, falling back to the
 * AGC-hosted statement when the platform reports none (hosting not configured,
 * or an unlisted debug build without the `appgallery_privacy_*` metadata) so the
 * user always has a way to read the policy.
 *
 * `AppPrivacyLink.type` is not publicly documented; the mapping used here treats
 * `type === 1` as the privacy statement when such an entry exists and otherwise
 * takes the first link (the statement is always present when hosting is in use),
 * labelling any remaining entries as user agreements. Refine here if a device
 * ever reports a different type scheme.
 */
export function platformPrivacyLinks(state: OhosPrivacyState | null, language: string): OhosPrivacyLinkEntry[] {
  const links = (state?.links ?? []).filter(
    (link): link is OhosPrivacyLink & { url: string } => typeof link.url === 'string' && link.url.length > 0,
  );
  if (links.length === 0) {
    return [{ labelKey: 'settings.about.privacyPolicy', url: privacyStatementUrl(language) }];
  }
  const statementIndex = links.findIndex((link) => link.type === 1);
  const primaryIndex = statementIndex >= 0 ? statementIndex : 0;
  return links.map((link, index) => ({
    labelKey:
      index === primaryIndex ? ('settings.about.privacyPolicy' as const) : ('settings.about.userAgreement' as const),
    url: link.url,
  }));
}
