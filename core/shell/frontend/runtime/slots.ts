// What an owner's `frontend/feature.ts` declares, and what the hosts read from it: one module
// per slot under `slots/`, each adding what it holds to `FrontendFeature`, gathered here. Only
// app/frontend lists the manifests; it installs them here before the first render, so no host
// imports a feature. What a feature or a connection adds to another's page, a tab of a page or
// a card on Connections, is there only while the DSP has it: switched off, or hidden from the
// DSP's members, it is gone, as are its pages and its sections of Settings.
//
// A manifest is loaded up front, and outside a browser by the route table's tests. It stays
// small, imports no CSS, touches no browser API until it is called, and loads its pages lazily.
//
// Adding a slot: its module under `slots/`, with what an owner puts in it, its field on
// `FrontendFeature` by declaration merging, and what the hosts read it with; then its names
// here.
export type { FrontendFeature } from './slots/installed.js';
export { installFeatures } from './slots/installed.js';
export type {
  Access,
  DspPageContext,
  DspPages,
  DspRoute,
  DspRouteId,
  PageContext,
  PlatformPages,
  PlatformRoute,
  PlatformRouteId,
  Route,
  RoutePrefetch,
  Sidebar,
  SidebarProps,
} from './slots/pages.js';
export {
  dspSetup,
  featureIcon,
  landingPage,
  listed,
  loadedSidebar,
  loadSidebar,
  movedTo,
  ownerPlace,
  pageSidebar,
  pagesUnder,
  routeOf,
  settingsPage,
  sidebarItems,
} from './slots/pages.js';
export type { CustomSection, CustomSectionContext, SettingsContext } from './slots/settings.js';
export { settingsRead, settingsSection } from './slots/settings.js';
export type {
  ConnectionCard,
  ConnectionCardContext,
  ConnectionPiece,
  ConnectionSection,
  ConnectionSectionContext,
  NewConnection,
} from './slots/connections.js';
export {
  connectionCard,
  connectionCards,
  connectionPieces,
  connectionSections,
} from './slots/connections.js';
export type {
  ConnectionIntegration,
  IntegrationNote,
  ListedIntegration,
} from './slots/integrations.js';
export { connectionIntegrations } from './slots/integrations.js';
export type { PageTab } from './slots/page-tabs.js';
export { pageTabs } from './slots/page-tabs.js';
export type { AccountAction } from './slots/account.js';
export { accountActions } from './slots/account.js';
export type { CacheRules } from './slots/api.js';
export {
  cacheRules,
  errorLabelOf,
  isLongPoll,
  loadReplyChecks,
  scheduleIssueOf,
} from './slots/api.js';
export type {
  Collected,
  CollectedField,
  CollectedPeriod,
  CollectedRecords,
} from './slots/collected.js';
export { collectedOf } from './slots/collected.js';
export type {
  AuditPart,
  AuditPhrases,
  AuditWording,
  AuditWords,
  PlatformSlots,
  RunWording,
} from './slots/platform.js';
export {
  auditWording,
  loadPlatformSlots,
  mailKindLabel,
  runWording,
  switchIcon,
} from './slots/platform.js';
export type { Watcher } from './slots/watchers.js';
export { watchers } from './slots/watchers.js';
