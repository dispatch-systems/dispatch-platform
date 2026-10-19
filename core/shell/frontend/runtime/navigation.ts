import {
  landingPage,
  movedTo,
  routeOf,
  readSettings,
  settingsPage,
  sidebarOwner,
  type Access,
  type DspRouteId,
  type PlatformRouteId,
} from './slots.js';
import { hasFeature } from './features.js';
import { siteDsp } from './site.js';
import type { DspView } from '../../../accounts/api/index.js';

const destinations = new Map<string, DspRouteId>();
export function rememberDestination(dspId: string, page: DspRouteId) {
  if (routeOf('dsp', page)?.remembered === false) return;
  destinations.delete(dspId);
  destinations.set(dspId, page);
  while (destinations.size > 100) destinations.delete(destinations.keys().next().value!);
}
export const clearDestinations = () => destinations.clear();
/** Drops the remembered page if it is `page`. */
export function forgetDestination(dspId: string, page: string) {
  if (destinations.get(dspId) === page) destinations.delete(dspId);
}

/** A DSP's address, naming no page. At the DSP's own address, the address already names it. */
const dspAddress = (dspId: string) => (dspId === siteDsp() ? '#' : `#dsp/${dspId}/`);
/**
 * A DSP's page, by default the last one open there, or else its landing page. Without a
 * landing page the address names no page, and the app opens the first one the person may.
 * At the DSP's own address, the address already names the DSP, so the page alone does.
 */
export const dspHash = (
  dspId: string,
  page: DspRouteId | undefined = destinations.get(dspId) ?? landingPage(),
  query?: Record<string, string>,
) => `${dspAddress(dspId)}${page ?? ''}${query ? `?${new URLSearchParams(query)}` : ''}`;
/**
 * A DSP's Settings, on `page` if one is named (`?tab=<page>`): the page that draws the settings.
 * None without that page, so a link to it is left out. Which of its pages a person has is the
 * server's to say, so a link names the page whether or not they have it, and Settings opens on
 * its first page for one who hasn't; given the person's view, it is left out once Settings has
 * been read and is known not to have it.
 */
export function settingsHash(dspId: string, page?: string, view?: DspView) {
  const host = settingsPage();
  if (!host) return undefined;
  if (page === undefined) return dspHash(dspId, host);
  const pages = view?.dsp.id === dspId ? readSettings()?.pages : undefined;
  if (pages && !pages.some((each) => each.id === page)) return undefined;
  return dspHash(dspId, host, { tab: page });
}
/**
 * A page of the DSP's, as another owner's, by its id: none while the DSP lacks it or the
 * person may not open it, so a link to it is left out.
 */
export function pageHash(
  access: Access & { view: DspView },
  page: string,
  query?: Record<string, string>,
) {
  const route = routeOf('dsp', page);
  if (
    !route ||
    (route.feature && !hasFeature(access.view, route.feature)) ||
    (route.permission && !route.permission(access))
  )
    return undefined;
  return dspHash(access.view.dsp.id, route.id, query);
}

// A page that draws a sidebar of its own, as Settings, has a way back at its top: to the
// address the member opened it from, within the same DSP. Moving among the pages under it
// keeps it; leaving the DSP, or another view of it opening, forgets it.
const waysBack = new Map<string, string>();
/** Notes the way back as the address moves `from` one hash `to` another. */
export function noteWayBack(from: string, to: string) {
  const before = parseHash(from);
  const after = parseHash(to);
  if (before.dspId !== after.dspId) waysBack.clear();
  else if (after.dspId) {
    const owner = sidebarOwner('dsp', after.page);
    if (owner && sidebarOwner('dsp', before.page) !== owner) waysBack.set(after.dspId, from);
  }
}
export const forgetWaysBack = () => waysBack.clear();
/**
 * Where a page's own sidebar leads back to in a DSP: the address it was opened from, or else,
 * opened at its own address, the DSP's last page outside it, or its landing page.
 */
export function wayBack(dspId: string) {
  const way = waysBack.get(dspId);
  if (way) return way;
  const last = destinations.get(dspId);
  const page = last && !sidebarOwner('dsp', last) ? last : landingPage();
  return page ? dspHash(dspId, page) : dspAddress(dspId);
}

export const platformHash = (page: PlatformRouteId = 'dsps', query?: Record<string, string>) =>
  `#${page}${query ? `?${new URLSearchParams(query)}` : ''}`;
export const signInHash = '#signin';

export function navigate(hash: string) {
  window.location.hash = hash;
}
/** Signing in, or following an invitation or reset link: outside any DSP, at every address. */
const signedOut = /^(?:signin$|invite\?|reset\?)/;
/**
 * Where the address points: `#dsp/<id>/<page>?…` inside a DSP, `#<page>?…` outside one. At a
 * DSP's own address, every `#<page>?…` is that DSP's but signing in and the links mailed there.
 */
export function parseHash(hash: string) {
  const own = siteDsp();
  if (own && !signedOut.test(hash.replace(/^#/, ''))) {
    const route = hash.replace(/^#/, '');
    return { route, dspId: own, page: (route || landingPage() || '').split('?')[0]! };
  }
  const route = hash.replace(/^#/, '') || 'dsps';
  const dspId = route.startsWith('dsp/') ? route.split('/')[1] : undefined;
  return {
    route,
    dspId,
    page: (dspId ? route.split('/')[2] || (landingPage() ?? '') : route).split('?')[0]!,
  };
}
export const hashQuery = () => new URLSearchParams(window.location.hash.split('?')[1]);
/**
 * Moves a link to where a page once was, as a tab of Settings it was, to the page, in place
 * in the browser's history, so the app reads the address the page is at now.
 */
export function followMoved() {
  const { dspId, page } = parseHash(location.hash);
  const moved = dspId && movedTo(page, hashQuery());
  if (moved)
    history.replaceState(
      history.state,
      '',
      `${location.pathname}${location.search}${dspHash(dspId, moved)}`,
    );
}
const linkTokens: { invite?: string; reset?: string } = {};
/**
 * A one-time link's secret, removed from browser history as soon as it is read. React may
 * render the screen that reads it again, or throw a first render away and render it anew, so
 * the secret is remembered while the address stays at the link's page, and forgotten once it
 * leaves.
 */
export function consumeHashToken(kind: 'invite' | 'reset') {
  if (window.location.hash.startsWith(`#${kind}?`)) {
    linkTokens[kind] = hashQuery().get('token') ?? '';
    history.replaceState(history.state, '', `${location.pathname}${location.search}#${kind}`);
  } else if (window.location.hash !== `#${kind}`) linkTokens[kind] = undefined;
  return linkTokens[kind];
}
/** Records a page's own state in the address without navigating. */
export function replaceHashQuery(query: Record<string, string>) {
  const search = new URLSearchParams(query).toString();
  history.replaceState(
    {},
    '',
    `${location.pathname}${location.search}${location.hash.split('?')[0]}${search ? `?${search}` : ''}`,
  );
}
