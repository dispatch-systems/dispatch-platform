import { hashQuery } from '../../../shell/frontend/runtime/navigation.js';
import { profileTab, securityTab, themeTab } from '../../../accounts/frontend/settings/tabs.js';

/** The platform owner's Settings tabs, in order: for now the account's own panels. */
export const tabs = [profileTab, securityTab, themeTab] as const;
export const tabOf = (id: string) => tabs.find((tab) => tab.id === id);

/** The code of the tab the address names, so the page opens on it. */
export const preloadTab = () =>
  tabOf(hashQuery().get('tab') || tabs[0].id)?.load() ?? Promise.resolve();
