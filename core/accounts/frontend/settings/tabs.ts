import { createElement, lazy } from 'react';
import type { SettingsTab } from '../../../shell/frontend/runtime/slots.js';

// The account's own Settings panels. A DSP's Settings shows them around its features' tabs;
// the platform owner's Settings shows them alone.

const loadProfile = () => import('./ProfileBadge.js');
const loadSecurity = () => import('./SecuritySettings.js');
const loadTheme = () => import('./ThemeSection.js');
const ProfileBadge = lazy(() => loadProfile().then((m) => ({ default: m.ProfileBadge })));
const SecuritySettings = lazy(() => loadSecurity().then((m) => ({ default: m.SecuritySettings })));
const ThemeSection = lazy(() => loadTheme().then((m) => ({ default: m.ThemeSection })));

export const profileTab: SettingsTab = {
  // The id stays `general` so existing links to the tab keep working.
  id: 'general',
  label: 'Profile',
  load: loadProfile,
  render: ({ session, view }) => createElement(ProfileBadge, { session, view }),
};
export const securityTab: SettingsTab = {
  id: 'security',
  label: 'Security',
  load: loadSecurity,
  render: () => createElement(SecuritySettings),
};
export const themeTab: SettingsTab = {
  id: 'theme',
  label: 'Theme',
  load: loadTheme,
  render: ({ session }) => createElement(ThemeSection, { userId: session.user.id }),
};
