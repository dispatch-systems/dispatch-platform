import { Award } from 'lucide-react';
import type { PlatformSlots } from '../../../core/shell/frontend/runtime/slots.js';
import { wording } from './audit-wording.js';

// What Weekly Scorecard puts in the platform owner's slots, loaded with the platform owner's pages.
export const slots: PlatformSlots = {
  switch: { id: 'weekly_scorecard', icon: Award },
  auditWording: wording,
};
