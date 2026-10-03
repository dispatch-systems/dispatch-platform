import type { ComponentType, ReactNode } from 'react';
import type { LucideIcon } from 'lucide-react';
import type { AgentArea, AgentSource, AuditEvent } from '../../../platform_owner/api/index.js';
import type { CollectionChange, JobMetrics } from '../../../collection/api/index.js';
import type { ConnectionFeature, Feature, PageFeature } from '../../../tenancy/api/index.js';
import type { DspView, SessionView } from '../../../accounts/api/index.js';
import type { Replies } from '../../../foundation/api/runtime.js';

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
  /** Whether opening the DSP again returns here, as to the last page open; omitted, it does. */
  remembered?: boolean;
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

/**
 * A tab one feature adds to another's page. The page draws it among its own tabs, as Settings
 * draws its settings tabs.
 */
export type PageTab = {
  /** The page it is a tab of. */
  page: DspRouteId;
  /** The tab's address on the page, `?tab=<id>`. It never changes, so links to it keep working. */
  id: string;
  label: string;
  /** Where the tab sits among the page's tabs, lowest first. */
  order: number;
  /** Loads the tab's code. */
  load: () => Promise<unknown>;
  render: (context: DspPageContext) => ReactNode;
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
  /** What its events add to their second line, after the log's own notes. */
  notes?: (event: AuditEvent) => string[];
  /** How a collection's outcome names it, as "<name> collection", by the connection that ran it. */
  collected?: Record<string, string>;
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

/** What a connection's card is drawn with on a DSP's Connections page. */
export type ConnectionCardContext = { development: boolean; timezone: string };
/** A collector's card on a DSP's Connections page. */
export type ConnectionCard = {
  /** The connection's catalog id. */
  provider: ConnectionFeature;
  /** The read the card shows, warmed while the Connections tab is about to open. */
  read: string;
  /** Loads the card's code. */
  load: () => Promise<unknown>;
  render: (context: ConnectionCardContext) => ReactNode;
};

/** A collection a collector runs, as Diagnostics and the audit log name it. */
export type CollectionLabels = {
  /** Its job kind. */
  kind: string;
  /** Its schedules' collection, and how the audit log names it. */
  schedule: { id: string; label: string };
  /** One item of its workload, for the per-item comparison. */
  unit: string;
  /** How many items a run's measurements counted. */
  count: (metrics: JobMetrics) => number | null;
};

/** How the response cache keeps an owner's reads current, each read named by its path prefix. */
export type CacheRules = {
  /** Its reads that hold collected data. */
  collected?: readonly string[];
  /** Whether a finished collection's changes reach one of its `collected` reads; omitted, any do. */
  collection?: (url: string, changes: readonly CollectionChange[]) => boolean;
  /** Its reads that change as a job starts, runs or ends. */
  jobs?: readonly string[];
  /** Its reads that change with the DSP's connections. */
  connections?: readonly string[];
  /**
   * Whether a write changes a read, for the writes it knows; undefined for the others. A read
   * changes when any owner says so, and a write an owner knows changes nothing else.
   */
  write?: (write: string, url: string) => boolean | undefined;
};

/**
 * What an owner puts in the platform owner's slots. Only the platform owner's pages read them,
 * so those pages load every owner's with them and no other page carries them.
 */
export type PlatformSlots = {
  /** Its page's switch, as the DSPs page lists it. */
  switch?: { id: PageFeature; icon: LucideIcon };
  /** How its events read in the audit log. */
  auditWording?: AuditWording;
  /** The kinds of its data agents may read. */
  readToggles?: ReadToggles;
  /** The collections it runs. */
  collections?: readonly CollectionLabels[];
  /** How a page that needs a capability its connection provides names it: "a … source". */
  capabilities?: Record<string, string>;
};

/** An owner's frontend: what it puts in each slot. */
export type FrontendFeature = {
  /** The owner's directory name. */
  name: string;
  /** Its pages, in sidebar order. */
  routes?: readonly Route[];
  /** Its tabs on a DSP's Settings page. */
  settingsTabs?: readonly SettingsTab[];
  /** Its tabs on another feature's page. */
  pageTabs?: readonly PageTab[];
  /** Loads its module that exports what it puts in the platform owner's slots, as `slots`. */
  platformSlots?: () => Promise<{ slots: PlatformSlots }>;
  /** Its connection's card. */
  connectionCard?: ConnectionCard;
  /** How the response cache treats its reads. */
  cache?: CacheRules;
  /** Its reads that wait for a change before they answer, by path prefix. */
  longPolls?: readonly string[];
  /** How the API client checks the replies of its routes, as core checks its own. */
  replies?: Replies;
  /** What its error codes say. */
  errors?: Record<string, string>;
  /** Why a schedule of its collections waits, by the code it last stopped on; its error too. */
  scheduleIssues?: Record<string, string>;
};

let installed: readonly FrontendFeature[] = [];
/** Called once by the app, before the first render, with every owner's manifest. */
export function installFeatures(features: readonly FrontendFeature[]) {
  installed = features;
  loadedSlots = [];
  slotsLoad = undefined;
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

/** The tabs other features add to `page`, in their order. */
export const pageTabs = (page: string) =>
  installed
    .flatMap((feature) => feature.pageTabs ?? [])
    .filter((tab) => tab.page === page)
    .sort((a, b) => a.order - b.order);

/** Every collector's connection card, in the order the collectors are listed. */
export const connectionCards = () =>
  installed.flatMap((feature) => (feature.connectionCard ? [feature.connectionCard] : []));
/** The card of a connection. */
export const connectionCard = (provider: string) =>
  connectionCards().find((card) => card.provider === provider);

/** Every owner's cache rules, in the order the owners are listed. */
export const cacheRules = () => installed.flatMap((feature) => feature.cache ?? []);

/** Every owner's reply validators, in the order the owners are listed. */
export const replyChecks = () => installed.flatMap((feature) => feature.replies ?? []);

/** Whether a read waits for a change before it answers. */
export const isLongPoll = (url: string) =>
  installed.some((feature) => feature.longPolls?.some((prefix) => url.startsWith(prefix)));

/** The first answer an owner gives, in the order the owners are listed. */
function first(read: (feature: FrontendFeature) => string | undefined) {
  for (const feature of installed) {
    const answer = read(feature);
    if (answer !== undefined) return answer;
  }
  return undefined;
}
/** What an owner's error code says. */
export const errorLabelOf = (code: string) =>
  first((feature) => feature.errors?.[code] ?? feature.scheduleIssues?.[code]);
/** Why a schedule of an owner's collections waits. */
export const scheduleIssueOf = (code: string) => first((feature) => feature.scheduleIssues?.[code]);

let loadedSlots: readonly (PlatformSlots & { owner: string })[] = [];
let slotsLoad: Promise<void> | undefined;
/**
 * Loads what every owner puts in the platform owner's slots, once; a failed load is tried again
 * next time. The readers below find nothing until it has loaded.
 */
export function loadPlatformSlots() {
  slotsLoad ??= Promise.all(
    installed.flatMap(({ name, platformSlots }) =>
      platformSlots ? [platformSlots().then(({ slots }) => ({ ...slots, owner: name }))] : [],
    ),
  ).then(
    (slots) => void (loadedSlots = slots),
    (error: unknown) => {
      slotsLoad = undefined;
      throw error;
    },
  );
  return slotsLoad;
}

/** The icon of a page's switch. */
export const switchIcon = (id: string) =>
  loadedSlots.find((slots) => slots.switch?.id === id)?.switch?.icon;

/** Every owner's audit wording, in the order the owners are listed. */
export const auditWording = () => loadedSlots.flatMap((slots) => slots.auditWording ?? []);

/** Every owner's kinds of data agents may read, group by group in their order. */
export const readToggles = () =>
  loadedSlots
    .flatMap((slots) => (slots.readToggles ? [slots.readToggles] : []))
    .sort((a, b) => a.order - b.order);

/** Every collection, with the collector that runs it. */
export const collectionLabels = () =>
  loadedSlots.flatMap((slots) =>
    (slots.collections ?? []).map((collection) => ({ ...collection, provider: slots.owner })),
  );

/** How a page that needs a capability names it, as the first connection listed names it. */
export const capabilityLabelOf = (capability: string) =>
  loadedSlots.find((slots) => slots.capabilities?.[capability])?.capabilities?.[capability];
