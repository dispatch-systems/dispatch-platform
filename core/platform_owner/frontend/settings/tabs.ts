import { hashQuery } from '../../../shell/frontend/runtime/navigation.js';
import {
  profileSection,
  securitySection,
  themeSection,
} from '../../../accounts/frontend/settings/sections.js';

/**
 * The platform owner's Settings tabs, in order: for now the account's own sections, each a tab
 * at the address a DSP's Settings gives its page.
 */
export const tabs = [
  { id: 'general', label: 'Profile', section: profileSection },
  { id: 'security', label: 'Security', section: securitySection },
  { id: 'theme', label: 'Theme', section: themeSection },
] as const;
export const tabOf = (id: string) => tabs.find((tab) => tab.id === id);

/** The code of the tab the address names, so the page opens on it. */
export const preloadTab = () =>
  tabOf(hashQuery().get('tab') || tabs[0].id)?.section.load() ?? Promise.resolve();
