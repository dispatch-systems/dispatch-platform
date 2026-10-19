import { createElement, lazy } from 'react';
import type { CustomSection } from '../../../shell/frontend/runtime/slots.js';

// The account's own Settings sections, each its member's own: core declares them on a DSP's
// Settings, on its Profile, Security and Theme pages, and the platform owner's Settings shows
// them alone, a tab each.

const loadProfile = () => import('./ProfileBadge.js');
const loadSecurity = () => import('./SecuritySettings.js');
const loadTheme = () => import('./ThemeSection.js');
const ProfileBadge = lazy(() => loadProfile().then((m) => ({ default: m.ProfileBadge })));
const SecuritySettings = lazy(() => loadSecurity().then((m) => ({ default: m.SecuritySettings })));
const ThemeSection = lazy(() => loadTheme().then((m) => ({ default: m.ThemeSection })));

export const profileSection: CustomSection = {
  id: 'accounts.profile',
  load: loadProfile,
  render: ({ session, view }) => createElement(ProfileBadge, { session, view }),
};
export const securitySection: CustomSection = {
  id: 'accounts.security',
  load: loadSecurity,
  render: () => createElement(SecuritySettings),
};
export const themeSection: CustomSection = {
  id: 'accounts.theme',
  load: loadTheme,
  render: ({ session }) => createElement(ThemeSection, { userId: session.user.id }),
};
