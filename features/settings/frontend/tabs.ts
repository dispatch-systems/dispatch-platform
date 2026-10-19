import type { DspView } from '../../../core/accounts/api/index.js';
import type { SettingsAnswer } from '../../../core/settings/api/index.js';
import { getSettings } from '../../../core/settings/api/client.js';
import { dataCache } from '../../../core/shell/frontend/runtime/data-cache.js';
import { hashQuery, parseHash } from '../../../core/shell/frontend/runtime/navigation.js';
import { admitted, warm } from '../../../core/shell/frontend/runtime/route-prefetch.js';
import {
  readSettings,
  settingsRead,
  settingsSection,
} from '../../../core/shell/frontend/runtime/slots.js';

// A DSP's Settings is what the server says the member has: its pages, each a tab, and the
// sections on them, which the page draws from their declarations. A custom section is drawn by
// the component its owner registers, and only while it has one with something to show.

/** The sections the page draws, in their order on their pages, as the server lists them. */
export const drawnSections = (settings: SettingsAnswer, view: DspView) =>
  settings.sections.filter((section) => {
    if (!section.custom) return true;
    const drawn = settingsSection(section.id);
    return Boolean(drawn && (!drawn.visible || drawn.visible(view)));
  });
/** The pages with a section drawn on them, in the order the server lists them. */
export const drawnPages = (settings: SettingsAnswer, view: DspView) => {
  const sections = drawnSections(settings, view);
  return settings.pages.filter((page) => sections.some((section) => section.page === page.id));
};
/** The page the page opens on: the one `requested`, while it is drawn, or else the first. */
export function openPage(settings: SettingsAnswer, view: DspView, requested: string | null) {
  const pages = drawnPages(settings, view);
  return (pages.find((page) => page.id === requested) ?? pages[0])?.id;
}

/** The sections drawn on a page, in their order. */
export const sectionsOn = (settings: SettingsAnswer, view: DspView, page: string | undefined) =>
  drawnSections(settings, view).filter((section) => section.page === page);
/** The custom sections of a page, as their owners register them. */
const customOn = (settings: SettingsAnswer, view: DspView, page: string | undefined) =>
  sectionsOn(settings, view, page).flatMap((section) =>
    section.custom ? (settingsSection(section.id) ?? []) : [],
  );
/** Loads the code of a page's custom sections. */
export const loadPage = (settings: SettingsAnswer, view: DspView, page: string | undefined) =>
  Promise.all(customOn(settings, view, page).map((section) => section.load()));

/** The page the address names, while it names this DSP's Settings. */
function addressed(view: DspView) {
  const address = parseHash(location.hash);
  return address.dspId === view.dsp.id && address.page === 'settings'
    ? hashQuery().get('tab')
    : null;
}
/**
 * Reads the member's Settings and loads the code of the page the address names, so the page
 * opens whole.
 */
export function preloadSettings(view?: DspView) {
  if (!admitted(view)) return Promise.resolve();
  return dataCache
    .read(settingsRead, getSettings)
    .then((settings) => loadPage(settings, view, openPage(settings, view, addressed(view))))
    .catch(() => undefined);
}
/** The reads a page's custom sections open on, warmed when the page may open next. */
export function prefetchPage(page: string, view: DspView, immediate = false) {
  const settings = readSettings();
  if (!settings || !admitted(view)) return;
  const reads = customOn(settings, view, page).flatMap((section) => section.prefetch?.(view) ?? []);
  if (reads.length) warm(reads, 'route:settings', immediate);
}
/** The member's Settings, and once it is read, the reads of the page the address names. */
export function prefetchSettings(view: DspView, immediate: boolean) {
  if (!admitted(view)) return;
  warm([settingsRead], 'route:settings', immediate);
  const settings = readSettings();
  const page = settings && openPage(settings, view, addressed(view));
  if (page) prefetchPage(page, view, immediate);
}
