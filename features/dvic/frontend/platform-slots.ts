import { ClipboardCheck } from 'lucide-react';
import type { PlatformSlots } from '../../../core/shell/frontend/runtime/slots.js';
import { wording } from './audit-wording.js';

// What DVIC puts in the platform owner's slots, loaded with the platform owner's pages.
export const slots: PlatformSlots = {
  switch: { id: 'dvic', icon: ClipboardCheck },
  auditWording: wording,
};
