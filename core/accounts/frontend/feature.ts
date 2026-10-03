import type { FrontendFeature } from '../../shell/frontend/runtime/slots.js';
import { profileTab, securityTab, themeTab } from './settings/tabs.js';

export const feature: FrontendFeature = {
  name: 'accounts',
  settingsTabs: [profileTab, securityTab, themeTab],
};
