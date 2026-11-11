import type { DspView, Permission } from '../../core/accounts/api/index.js';
import { installCustomCatalog } from '../../core/shell/frontend/runtime/features.js';
import { installCustomPermissions } from '../../core/shell/frontend/runtime/permissions.js';
import {
  installCustomPeople,
  type PeopleDescriptors,
} from '../../core/shell/frontend/runtime/people.js';
import {
  installCustomFeatures,
  type FrontendFeature,
} from '../../core/shell/frontend/runtime/slots.js';

type Loaded = PeopleDescriptors & {
  feature: FrontendFeature;
  schedulingPermissions?: readonly Permission[];
};
let pending = new AbortController();
const styles = new Set<HTMLLinkElement>();

/** A workspace boundary removes both admitted and still-loading private UI. */
export function clearCustomFeatures() {
  pending.abort();
  pending = new AbortController();
  for (const style of styles) style.remove();
  styles.clear();
  // Vite may have loaded a private page's CSS along with its lazy component.
  document.querySelectorAll<HTMLLinkElement>('link[rel="stylesheet"]').forEach((style) => {
    if (new URL(style.href).pathname.startsWith('/assets/custom-')) style.remove();
  });
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
    style.onerror = () => done(new Error('custom_styles_unavailable'));
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
  const modules = await Promise.all(
    entries.map(async (entry): Promise<Loaded> => {
      const loaded: Loaded = await import(/* @vite-ignore */ entry.url);
      if (loaded.feature?.name !== entry.name) throw new Error('invalid_custom_feature');
      return loaded;
    }),
  );
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
    for (const style of css) style.media = 'all';
  };
}
