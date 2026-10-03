import type { ComponentType, ReactNode } from 'react';
import type { LucideIcon } from 'lucide-react';
import type {
  AgentArea,
  AgentSource,
  AuditEvent,
  DspView,
  Feature,
  SessionView,
} from '../../../../shared/contracts/index.js';

// What an owner's `frontend/feature.ts` declares, and what the hosts read from it. Only
// app/frontend lists the manifests; it installs them here before the first render, so no host
// imports a feature.
//
// A manifest is loaded up front, and outside a browser by the route table's tests. It stays
// small, imports no CSS, touches no browser API until it is called, and loads its pages lazily.

/** Every page inside a DSP, by id. Each owner's feature.ts adds its own by declaration merging. */
export interface DspPages {}
/** Every page outside a DSP, by id, added the same way. */
export interface PlatformPages {}
export type DspRouteId = Extract<keyof DspPages, string>;
export type PlatformRouteId = Extract<keyof PlatformPages, string>;

/** Who is asking: the session, and inside a DSP its admitted view. */
export type Access = { session: SessionView; view?: DspView };
export type PageContext = { session: SessionView };
export type DspPageContext = PageContext & { view: DspView; reopen: () => Promise<void> };
/** What a page's prefetch hook may do: warm its primary data before the page mounts. */
export type RoutePrefetch = {
  session?: SessionView;
  /** Read now, at high priority, rather than queue a warmup. */
  immediate: boolean;
  /** Warms these reads for the page. */
  warm: (urls: string[]) => void;
};

type Page<Context> = {
  /** The page's name in the sidebar, the breadcrumb and the document title. */
  label: string;
  /** The navigation item to highlight for a page that has none of its own. */
  parent?: string;
  icon?: LucideIcon;
  /** Whether the sidebar lists the page. */
  nav: boolean | ((access: Access) => boolean);
  /** Who may open the page; omitted means everyone in the scope. */
  permission?: (access: Access) => boolean;
  /** The feature the page belongs to; a DSP without it has no such page. */
  feature?: Feature;
  /** Loads the page's code, and with access what the page opens on. */
  preload: (access?: Access) => Promise<unknown>;
  render: (context: Context) => ReactNode;
};
export type DspRoute = Page<DspPageContext> & {
  id: DspRouteId;
  scope: 'dsp';
  /** A page rendered before, whose admitted data is cached, may commit before the next paint. */
  ready?: (view: DspView) => boolean;
  /** Called only for the admitted view. */
  prefetch?: (prefetch: RoutePrefetch & { view: DspView }) => void;
};
export type PlatformRoute = Page<PageContext> & {
  id: PlatformRouteId;
  scope: 'platform';
  /** Called only while no DSP view is admitted. */
  prefetch?: (prefetch: RoutePrefetch) => void;
};
export type Route = DspRoute | PlatformRoute;

/** What a Settings panel is drawn for: the person, and inside a DSP its view. */
export type SettingsContext = { session: SessionView; view?: DspView };
/** A tab of a Settings page. */
export type SettingsTab = {
  /** The tab's address, `?tab=<id>`. It never changes, so links to it keep working. */
  id: string;
  label: string;
  /**
   * Draws the label with more beside it, such as a count. Settings loads it with the page, as
   * part of the page's own code once was, so the tab never shows without it.
   */
  badge?: { load: () => Promise<ComponentType<{ active: boolean }>> };
  /** Where the tab sits among the page's tabs, lowest first. */
  order: number;
  /** Who sees the tab; omitted means everyone. */
  visible?: (view?: DspView) => boolean;
  /** Loads the panel's code. */
  load: () => Promise<unknown>;
  render: (context: SettingsContext) => ReactNode;
  /**
   * The reads to warm, for the admitted view, while `tab` is about to open. A tab may name its
   * badge's reads whichever tab that is.
   */
  prefetch?: (tab: string, view: DspView) => string[];
};

/** A part of an audit log sentence: plain words, or words to stress. */
export type AuditPart = string | { strong: string };
/** What the audit log writes its sentences with. */
export type AuditWords = {
  strong: (value: string) => AuditPart;
  /** A `YYYY-MM-DD` day as the log reads it: "Sep 3". */
  day: (value: string) => string;
  /** A `HH:MM` time of day as the log reads it: "10:01 AM". */
  clock: (value: string) => string;
};
/** Each event's sentence after the actor's name, by action. */
export type AuditPhrases = Record<string, (event: AuditEvent, words: AuditWords) => AuditPart[]>;
/** How an owner's events read in the platform owner's audit log. */
export type AuditWording = {
  phrases?: AuditPhrases;
  /** The actions whose sentence already says what the event's detail holds. */
  spoken?: readonly string[];
  /** The names of the fields its events change. */
  fields?: Record<string, string>;
  /** How a value of one of its fields reads; undefined leaves it to the log. */
  value?: (field: string, value: string, words: AuditWords) => string | undefined;
};

/** A kind of data agents may read, as the Agents page shows its switch. */
export type ReadToggle = {
  /** Permanent: keys, apps and the audit log store it. */
  id: AgentArea;
  label: string;
  /** What it holds, where its label alone doesn't say. */
  hint?: string;
  /** How a key's row names it when the key doesn't read it. */
  missing: string;
  /** The switch it is read from, which a DSP may have switched off. */
  source: AgentSource;
  /** The kind it comes with and only matters beside: it is allowed only with that one. */
  with?: AgentArea;
  /** A new key or app leaves it off. */
  optIn?: boolean;
};
/** An owner's kinds of data agents may read, under its name on the Agents page. */
export type ReadToggles = {
  label: string;
  /** How a key's row names the whole group when the key reads none of it. */
  missing: string;
  /** Where the group sits among the others, lowest first. */
  order: number;
  /** Each of its switches' names, said alone when only that one is off. */
  sources: Partial<Record<AgentSource, string>>;
  toggles: readonly ReadToggle[];
};

/** An owner's frontend: what it puts in each slot. */
export type FrontendFeature = {
  /** The owner's directory name. */
  name: string;
  /** Its pages, in sidebar order. */
  routes?: readonly Route[];
  /** Its tabs on a DSP's Settings page. */
  settingsTabs?: readonly SettingsTab[];
  /** Loads how its events read in the audit log. */
  auditWording?: () => Promise<AuditWording>;
  /** The kinds of its data agents may read. */
  readToggles?: ReadToggles;
};

let installed: readonly FrontendFeature[] = [];
/** Called once by the app, before the first render, with every owner's manifest. */
export function installFeatures(features: readonly FrontendFeature[]) {
  installed = features;
}

/** The installed page at an address. */
export function routeOf(scope: 'dsp', page: string): DspRoute | undefined;
export function routeOf(scope: 'platform', page: string): PlatformRoute | undefined;
export function routeOf(scope: Route['scope'], page: string): Route | undefined {
  for (const feature of installed)
    for (const route of feature.routes ?? [])
      if (route.scope === scope && route.id === page) return route;
  return undefined;
}

/** Every owner's tabs on a DSP's Settings page, in the order the owners are listed. */
export const settingsTabs = () => installed.flatMap((feature) => feature.settingsTabs ?? []);

/** Every owner's kinds of data agents may read, group by group in their order. */
export const readToggles = () =>
  installed
    .flatMap((feature) => (feature.readToggles ? [feature.readToggles] : []))
    .sort((a, b) => a.order - b.order);

let wordingLoad: Promise<readonly AuditWording[]> | undefined;
/** Loads every owner's audit wording, once; a failed load is tried again next time. */
export function loadAuditWording() {
  wordingLoad ??= Promise.all(
    installed.flatMap((feature) => (feature.auditWording ? [feature.auditWording()] : [])),
  ).catch((error: unknown) => {
    wordingLoad = undefined;
    throw error;
  });
  return wordingLoad;
}
