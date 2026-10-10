import type { ComponentType, ReactNode } from 'react';
import type { LucideIcon } from 'lucide-react';
import type { AuditEvent } from '../../../platform_owner/api/index.js';
import type { CollectionChange, JobMetrics, PageReads } from '../../../collection/api/index.js';
import type { ConnectionFeature, Feature, PageFeature } from '../../../tenancy/api/index.js';
import type { DspView, Permission, SessionView } from '../../../accounts/api/index.js';
import type { Replies } from '../../../foundation/api/runtime.js';
import { featureCatalog, hasFeature } from './features.js';

// What an owner's `frontend/feature.ts` declares, and what the hosts read from it. Only
// app/frontend lists the manifests; it installs them here before the first render, so no host
// imports a feature. What a feature or a connection adds to another's page, a Settings tab or
// a piece of one, a tab of a page, is there only while the DSP has it: switched off, or hidden
// from the DSP's members, it is gone, as are its pages.
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
  /** The page's name in the sidebar and the document title. */
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
  /** Whether a DSP opens here when it has no last page to return to; one page at most does. */
  landing?: boolean;
  /** Whether the page draws every owner's settings tabs, as a DSP's Settings; one at most does. */
  hostsSettings?: boolean;
  /** Called only for the admitted view. */
  prefetch?: (prefetch: RoutePrefetch & { view: DspView }) => void;
};
export type PlatformRoute = Page<PageContext> & {
  id: PlatformRouteId;
  scope: 'platform';
  /** The page the sidebar lists it after, one an owner listed before its own declares. */
  after?: PlatformRouteId;
  /**
   * Whether signing in at a link to it, one with a query, stays there, as for an app asking
   * to connect; else signing in opens the platform's first page.
   */
  linked?: boolean;
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
  /** The part of its owner's feature it belongs to, as `x.uploads`: it goes with the part. */
  part?: Feature;
  /** Loads the panel's code. */
  load: () => Promise<unknown>;
  render: (context: SettingsContext) => ReactNode;
  /**
   * The reads to warm, for the admitted view, while `tab` is about to open. A tab may name its
   * badge's reads whichever tab that is.
   */
  prefetch?: (tab: string, view: DspView) => string[];
};
/** A piece one owner adds to a Settings tab, drawn under the tab's own panel. */
export type SettingsPiece = {
  /** The tab it is drawn on. */
  tab: string;
  /** Its own id among the tab's pieces. */
  id: string;
  /** Where it sits among the tab's pieces, lowest first. */
  order: number;
  /** Who sees it; omitted means everyone who sees the tab. */
  visible?: (view?: DspView) => boolean;
  /** The part of its owner's feature it belongs to: it goes with the part. */
  part?: Feature;
  /** Loads its code; the tab waits for it before it opens. */
  load: () => Promise<unknown>;
  render: (context: SettingsContext) => ReactNode;
};

/**
 * A card a feature adds to a DSP's Connections tab, beside the collectors' own: one of the DSP's
 * accounts, which those with Manage DSP Connections manage, or the member's own account.
 */
export type ConnectionPiece = {
  /** Its own id among the section's cards. */
  id: string;
  /** `dsp` for one of the DSP's accounts, `personal` for the member's own. */
  section: 'dsp' | 'personal';
  /** Where it sits among the section's cards, after the collectors', lowest first. */
  order: number;
  /** Who sees it; omitted means everyone who sees the section. */
  visible?: (view?: DspView) => boolean;
  /** The part of its owner's feature it belongs to: it goes with the part. */
  part?: Feature;
  /** The reads it shows, warmed while the Connections tab is about to open. */
  reads: readonly string[];
  /** Loads its code; the tab loads every card's before it opens. */
  load: () => Promise<unknown>;
  render: (context: SettingsContext) => ReactNode;
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
  /** The part of its owner's feature it belongs to: it goes with the part. */
  part?: Feature;
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

/**
 * How Diagnostics words the measurements core keeps of every collection's runs, beyond their
 * timings and memory: what an attempt collected, and its page reads.
 */
export type RunWording = {
  /** What an attempt collected; undefined says nothing, and the row reads "—". */
  collected: (attempt: JobMetrics) => string | undefined;
  /** The rows its page reads add to the attempt's measurements. */
  reads: (reads: PageReads) => [label: string, value: string][];
  /** Draws its slow and failed reads, if it has any, below them. */
  slowReads: ComponentType<{ attempt: JobMetrics }>;
};

/** How the response cache keeps an owner's reads current, each read named by its path prefix. */
export type CacheRules = {
  /** Its reads that hold collected data. */
  collected?: readonly string[];
  /** Whether a finished collection's changes reach one of its `collected` reads; omitted, any do. */
  collection?: (url: string, changes: readonly CollectionChange[]) => boolean;
  /** Its reads that change as a job starts, runs or ends; a write under one starts or ends one. */
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
 * so those pages load every owner's with them and no other page carries them. Their labels,
 * such as the read toggles' and the collections', are generated from the backend instead.
 */
export type PlatformSlots = {
  /** Its page's switch, as the DSPs page lists it. */
  switch?: { id: PageFeature; icon: LucideIcon };
  /** How its events read in the audit log. */
  auditWording?: AuditWording;
  /** How Diagnostics words every collection's runs; the first owner listed with it does. */
  runWording?: RunWording;
  /** How the mail log names the kinds of email it sends a member, by kind. */
  mailKinds?: Record<string, string>;
};

/** An owner's frontend: what it puts in each slot. */
export type FrontendFeature = {
  /** The owner's directory name. */
  name: string;
  /** Its pages, in sidebar order. */
  routes?: readonly Route[];
  /** Its tabs on a DSP's Settings page. */
  settingsTabs?: readonly SettingsTab[];
  /** Its pieces of other owners' tabs on a DSP's Settings page. */
  settingsPieces?: readonly SettingsPiece[];
  /**
   * Who may save a DSP's profile, and the route it serves that saves it, when the owner saves
   * it: opening a DSP that has none asks them for it. One owner at most says so; without one,
   * nobody is asked.
   */
  dspSetup?: { permission: Permission; save: string };
  /** Its tabs on another feature's page. */
  pageTabs?: readonly PageTab[];
  /** Loads its module that exports what it puts in the platform owner's slots, as `slots`. */
  platformSlots?: () => Promise<{ slots: PlatformSlots }>;
  /** Its connection's card. */
  connectionCard?: ConnectionCard;
  /** Its cards on a DSP's Connections tab. */
  connectionPieces?: readonly ConnectionPiece[];
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

/** The DSP page that says so, the first listed if more do. */
function dspPage(says: (route: DspRoute) => boolean | undefined) {
  for (const feature of installed)
    for (const route of feature.routes ?? [])
      if (route.scope === 'dsp' && says(route)) return route.id;
  return undefined;
}
/** The page a DSP opens on, as a feature declares it; none without one. */
export const landingPage = () => dspPage((route) => route.landing);
/** The page that draws every owner's settings tabs, as a feature declares it; none without one. */
export const settingsPage = () => dspPage((route) => route.hostsSettings);

/** Who may save a DSP's profile when it is first opened, and where, as the owner that saves it says. */
export const dspSetup = () => installed.find((feature) => feature.dspSetup)?.dspSetup;

/**
 * What the owners put in a slot that the view has, in the order the owners are listed: all of
 * core's, a feature's or a connection's while the DSP has it, and a part's while it has that.
 */
function present<T extends { part?: Feature }>(
  view: DspView | undefined,
  slot: (feature: FrontendFeature) => readonly T[] | undefined,
) {
  return installed.flatMap((feature) =>
    featureCatalog.some((f) => f.id === feature.name) && !hasFeature(view, feature.name as Feature)
      ? []
      : (slot(feature) ?? []).filter((each) => !each.part || hasFeature(view, each.part)),
  );
}
/**
 * Every owner's tabs on a DSP's Settings page, in the order the owners are listed; with a view,
 * those it has.
 */
export const settingsTabs = (view?: DspView) =>
  view
    ? present(view, (feature) => feature.settingsTabs)
    : installed.flatMap((feature) => feature.settingsTabs ?? []);
/** The pieces of a Settings tab the view has and sees, in their order. */
export const settingsPieces = (tab: string, view: DspView | undefined) =>
  present(view, (feature) => feature.settingsPieces)
    .filter((piece) => piece.tab === tab && (!piece.visible || piece.visible(view)))
    .sort((a, b) => a.order - b.order);

/** The tabs other owners add to `page` that the view has, in their order. */
export const pageTabs = (page: string, view: DspView) =>
  present(view, (feature) => feature.pageTabs)
    .filter((tab) => tab.page === page)
    .sort((a, b) => a.order - b.order);

/** Every collector's connection card, in the order the collectors are listed. */
export const connectionCards = () =>
  installed.flatMap((feature) => (feature.connectionCard ? [feature.connectionCard] : []));
/**
 * The cards features add to a section of the Connections tab: with a view, those it has and
 * sees, in their order; without one, every one, for loading their code.
 */
export const connectionPieces = (section: ConnectionPiece['section'], view?: DspView) =>
  (view
    ? present(view, (feature) => feature.connectionPieces).filter(
        (piece) => !piece.visible || piece.visible(view),
      )
    : installed.flatMap((feature) => feature.connectionPieces ?? [])
  )
    .filter((piece) => piece.section === section)
    .sort((a, b) => a.order - b.order);
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

let loadedSlots: readonly PlatformSlots[] = [];
let slotsLoad: Promise<void> | undefined;
/**
 * Loads what every owner puts in the platform owner's slots, once; a failed load is tried again
 * next time. The readers below find nothing until it has loaded.
 */
export function loadPlatformSlots() {
  slotsLoad ??= Promise.all(
    installed.flatMap(({ platformSlots }) =>
      platformSlots ? [platformSlots().then(({ slots }) => slots)] : [],
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

/** How the mail log names an owner's kind of email, once the slots have loaded. */
export const mailKindLabel = (kind: string) =>
  loadedSlots.find((slots) => slots.mailKinds?.[kind])?.mailKinds?.[kind];
/** The icon of a feature's switch: the one its platform slots name, else its page's. */
export const switchIcon = (id: string) =>
  loadedSlots.find((slots) => slots.switch?.id === id)?.switch?.icon ??
  installed.find((feature) => feature.name === id)?.routes?.find((route) => route.icon)?.icon;

/** Every owner's audit wording, in the order the owners are listed. */
export const auditWording = () => loadedSlots.flatMap((slots) => slots.auditWording ?? []);

/** How Diagnostics words every collection's runs, as the first owner listed with it does. */
export const runWording = () => loadedSlots.find((slots) => slots.runWording)?.runWording;
