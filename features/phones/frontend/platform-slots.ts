import { Smartphone } from 'lucide-react';
import type { PlatformSlots } from '../../../core/shell/frontend/runtime/slots.js';
import { wording } from './audit-wording.js';

// What Phones puts in the platform owner's slots, loaded with the platform owner's pages.
export const slots: PlatformSlots = {
  switch: { id: 'phones', icon: Smartphone },
  auditWording: wording,
};
