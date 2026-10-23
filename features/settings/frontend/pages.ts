import type { DspView } from '../../../core/accounts/api/index.js';
import type { SettingsAnswer, SettingsPage } from '../../../core/settings/api/index.js';
import { getSettings, readSettings } from '../../../core/settings/api/client.js';
import { dataCache } from '../../../core/shell/frontend/runtime/data-cache.js';
import { hashQuery, parseHash } from '../../../core/shell/frontend/runtime/navigation.js';
import {
  admitted,
  prefetchRouteData,
  warm,
} from '../../../core/shell/frontend/runtime/route-prefetch.js';
import {
  ownerPlace,
  pageSidebar,
  pagesUnder,
  settingsPage,
  settingsRead,
  settingsSection,
  type Access,
  type DspRoute,
} from '../../../core/shell/frontend/runtime/slots.js';

// A DSP's Settings is what the server says the member has: its pages, which its sidebar lists,
// and the sections on them, which the page draws from their declarations. A custom section is drawn by
// the component its owner registers, and only while it has one with something to show. Its
// sidebar lists, under Features, the pages features put under it too, each a page of its own.

/**
 * A page Settings' sidebar lists: one of Settings', at `?tab=<id>`, or one a feature puts under
 * it (`route`), naming its sidebar its `parent`, at its own address.
 */
export type ListedPage = Pick<SettingsPage, 'id' | 'label' | 'group'> & { route?: DspRoute };

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
/** A page of Features' place there: its feature's, among the owners as the app lists them. */
const place = (page: ListedPage) =>
  ownerPlace(page.route?.feature ?? page.id.replaceAll('-', '_')) ?? Infinity;
/**
 * The pages Settings' sidebar lists, in their order: those it draws, and under Features, each in
 * its feature's place, the pages features put under it that the person may open.
 */
export function listedPages(settings: SettingsAnswer, access: Access & { view: DspView }) {
  const pages: ListedPage[] = drawnPages(settings, access.view);
  const host = settingsPage();
  const sidebar = host && pageSidebar('dsp', host);
  for (const route of sidebar ? pagesUnder(sidebar.id, access) : []) {
    const page: ListedPage = { id: route.id, label: route.label, group: 'features', route };
    const at = pages.findIndex((each) => each.group === 'features' && place(each) > place(page));
    pages.splice(at < 0 ? pages.length : at, 0, page);
  }
  return pages;
}
/** The page the page opens on: the one `requested`, while it is drawn, or else the first. */
export function openPage(settings: SettingsAnswer, view: DspView, requested: string | null) {
  const pages = drawnPages(settings, view);
  return (pages.find((page) => page.id === requested) ?? pages[0])?.id;
}

/**
 * The sections drawn on a page, in their order: those that go with a collection are drawn with it,
 * by the Collectors page.
 */
export const sectionsOn = (settings: SettingsAnswer, view: DspView, page: string | undefined) =>
  drawnSections(settings, view).filter((section) => section.page === page && !section.collection);
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
/** Warms a page a feature puts under Settings as the app's sidebar does: its code and first reads. */
export function warmRoute(route: DspRoute, access: Access & { view: DspView }) {
  void route.preload(access).catch(() => undefined);
  prefetchRouteData(route.id, access.view, access.session);
}
/** The member's Settings, and once it is read, the reads of the page the address names. */
export function prefetchSettings(view: DspView, immediate: boolean) {
  if (!admitted(view)) return;
  warm([settingsRead], 'route:settings', immediate);
  const settings = readSettings();
  const page = settings && openPage(settings, view, addressed(view));
  if (page) prefetchPage(page, view, immediate);
}
