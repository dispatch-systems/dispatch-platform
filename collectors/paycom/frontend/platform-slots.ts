import type { PlatformSlots } from '../../../core/shell/frontend/runtime/slots.js';
import { wording } from './audit-wording.js';

// What Paycom puts in the platform owner's slots, loaded with the platform owner's pages.
export const slots: PlatformSlots = {
  auditWording: wording,
};
