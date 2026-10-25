import { RadioTower } from 'lucide-react';
import type { PlatformSlots } from '../../../core/shell/frontend/runtime/slots.js';

// What Live Route Data puts in the platform owner's slots, loaded with the platform owner's pages.
export const slots: PlatformSlots = {
  switch: { id: 'live_route_data', icon: RadioTower },
};
