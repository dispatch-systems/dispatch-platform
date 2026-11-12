import type { DspView, Permission, SessionView } from '../../core/accounts/api/index.js';
import { installCustomCatalog } from '../../core/shell/frontend/runtime/features.js';
import { installCustomPermissions } from '../../core/shell/frontend/runtime/permissions.js';
import {
  installCustomPeople,
  type PeopleDescriptors,
} from '../../core/shell/frontend/runtime/people.js';
import {
  installCustomFeatures,
  installCustomPlatformFeatures,
  type FrontendFeature,
} from '../../core/shell/frontend/runtime/slots.js';

type Loaded = PeopleDescriptors & {
  feature: FrontendFeature;
  schedulingPermissions?: readonly Permission[];
};
const unavailable = () => new Error('Unable to open this DSP.');
let pending = new AbortController();
const styles = new Set<HTMLLinkElement>();
let activeSources = new Set<string>();
let activeStyles = new Set<string>();
let platformStyles = false;
const cachedStyles = new Map<string, HTMLLinkElement>();
let sourceObserver: MutationObserver | undefined;
const privateStyles = () =>
  [...document.querySelectorAll<HTMLLinkElement>('link[rel="stylesheet"]')].filter((style) =>
    new URL(style.href).pathname.startsWith('/assets/custom-'),
  );

/** Vite keeps source CSS in style tags and caches its modules. Retain those tags for HMR
 * and cached re-entry, but apply them only while their feature is admitted. */
function sourceStyles() {
  document.querySelectorAll<HTMLStyleElement>('style[data-vite-dev-id]').forEach((style) => {
    const file = style.dataset.viteDevId?.replaceAll('\\', '/') ?? '';
    const feature = /\/features\/custom\/([a-z0-9]+)\/([a-z][a-z0-9_]*)\//.exec(file);
    if (feature)
      style.media =
        platformStyles || activeSources.has(`${feature[1]}_${feature[2]}`) ? 'all' : 'not all';
  });
  privateStyles().forEach((style) => {
    // Admission stages its own styles with media disabled until its view commits.
    if (styles.has(style) && !activeStyles.has(style.href)) return;
    cachedStyles.set(style.href, style);
    style.media = platformStyles || activeStyles.has(style.href) ? 'all' : 'not all';
  });
  if (platformStyles)
    for (const style of cachedStyles.values()) {
      style.media = 'all';
      if (!style.isConnected) document.head.append(style);
    }
}
/** Platform slots can belong to any owner, but their styles apply only on platform pages. */
export function setPlatformCustomStyles(active: boolean) {
  platformStyles = active;
  watchSourceStyles();
  sourceStyles();
}
function watchSourceStyles() {
  if (sourceObserver) return;
  // A lazy page can finish importing CSS after its DSP was left. The observer also
  // catches those late Vite tags before they paint in the next workspace.
  sourceObserver = new MutationObserver(sourceStyles);
  sourceObserver.observe(document.head, { childList: true });
}

/** A workspace boundary removes both admitted and still-loading private UI. */
export function clearCustomFeatures() {
  pending.abort();
  pending = new AbortController();
  activeSources.clear();
  activeStyles.clear();
  watchSourceStyles();
  for (const style of styles) style.remove();
  styles.clear();
  // Vite may have loaded a private page's CSS along with its lazy component.
  privateStyles().forEach((style) => {
    cachedStyles.set(style.href, style);
    style.media = 'not all';
    // A lazy slot may still be waiting for this link's load event. Detaching it before
    // it finishes would leave the cached module's promise pending forever.
    if (style.sheet) style.remove();
  });
  sourceStyles();
  installCustomFeatures([]);
  installCustomCatalog();
  installCustomPermissions();
  installCustomPeople([]);
}

function prepareStyle(url: string, signal: AbortSignal): Promise<HTMLLinkElement> {
  return new Promise((resolve, reject) => {
    const style = document.createElement('link');
    style.rel = 'stylesheet';
    style.media = 'not all';
    style.href = url;
    const done = (error?: Error) => {
      signal.removeEventListener('abort', aborted);
      style.onload = style.onerror = null;
      if (error) {
        style.remove();
        styles.delete(style);
        reject(error);
      } else resolve(style);
    };
    const aborted = () => done(new DOMException('Workspace changed', 'AbortError'));
    style.onload = () => done();
    style.onerror = () => done(unavailable());
    if (signal.aborted) return aborted();
    signal.addEventListener('abort', aborted, { once: true });
    styles.add(style);
    document.head.append(style);
  });
}

/** Loads into a staging area. Only the caller's still-current admission commits it. */
export async function prepareCustomFeatures(view: DspView, current: () => boolean) {
  const signal = pending.signal;
  const entries = view.custom?.frontends ?? [];
  watchSourceStyles();
  const modules = await Promise.all(
    entries.map(async (entry): Promise<Loaded> => {
      const loaded = (await import(/* @vite-ignore */ entry.url)) as Loaded;
      if (loaded.feature?.name !== entry.name) throw unavailable();
      return loaded;
    }),
  ).catch(() => {
    sourceStyles();
    throw unavailable();
  });
  sourceStyles();
  if (!current() || signal.aborted) return;
  let css: HTMLLinkElement[];
  try {
    css = await Promise.all(
      [...new Set(entries.flatMap((entry) => entry.styles))].map((url) =>
        prepareStyle(url, signal),
      ),
    );
  } catch (error) {
    if (current() && !signal.aborted) clearCustomFeatures();
    throw error;
  }
  if (!current() || signal.aborted) return;
  return () => {
    installCustomCatalog(view.custom);
    installCustomPermissions(
      view.custom,
      modules.flatMap((module) => module.schedulingPermissions ?? []),
    );
    installCustomPeople(modules);
    installCustomFeatures(modules.map((module) => module.feature));
    activeSources = new Set(entries.map((entry) => entry.name));
    activeStyles = new Set(css.map((style) => style.href));
    for (const style of css) cachedStyles.set(style.href, style);
    sourceStyles();
    for (const style of css) style.media = 'all';
  };
}

type PlatformLoaded = {
  features: readonly FrontendFeature[];
  featureLabels: Readonly<Record<string, string>>;
  permissionLabels: Readonly<Record<string, string>>;
};
let platformGeneration = 0;

export function clearPlatformFeatures() {
  platformGeneration++;
  setPlatformCustomStyles(false);
  installCustomPlatformFeatures();
}

/** The server supplies this private entry only to verified platform owners. */
export async function preparePlatformFeatures(session: SessionView, current: () => boolean) {
  if (
    !session.user.platformOwner ||
    (session.security.required && !session.security.verified) ||
    !session.platformFrontend
  )
    return;
  const request = platformGeneration;
  const loaded = (await import(/* @vite-ignore */ session.platformFrontend)) as PlatformLoaded;
  if (!current() || request !== platformGeneration) return;
  return () =>
    installCustomPlatformFeatures(loaded.features, {
      features: loaded.featureLabels,
      permissions: loaded.permissionLabels,
    });
}
