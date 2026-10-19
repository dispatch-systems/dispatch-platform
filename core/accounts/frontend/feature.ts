import type { FrontendFeature } from '../../shell/frontend/runtime/slots.js';
import { profileSection, securitySection, themeSection } from './settings/sections.js';

export const feature: FrontendFeature = {
  name: 'accounts',
  settingsSections: [profileSection, securitySection, themeSection],
};
