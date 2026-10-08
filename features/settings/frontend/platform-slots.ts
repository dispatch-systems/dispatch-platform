import { Settings } from 'lucide-react';
import type { PlatformSlots } from '../../../core/shell/frontend/runtime/slots.js';

// What Settings puts in the platform owner's slots: its icon on the DSPs page.
export const slots: PlatformSlots = {
  switch: { id: 'settings', icon: Settings },
};
