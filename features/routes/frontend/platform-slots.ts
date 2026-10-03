import { Route } from 'lucide-react';
import type { PlatformSlots } from '../../../core/shell/frontend/runtime/slots.js';
import { wording } from './audit-wording.js';

// What Routes puts in the platform owner's slots, loaded with the platform owner's pages.
export const slots: PlatformSlots = {
  switch: { id: 'routes', icon: Route },
  auditWording: wording,
  readToggles: {
    label: 'Routes',
    missing: 'route data',
    order: 10,
    sources: { routes: 'Routes' },
    toggles: [
      { id: 'routes', label: 'Routes & packages', missing: 'routes', source: 'routes' },
      {
        id: 'locations',
        label: 'Delivery addresses & GPS',
        hint: 'Stop addresses and GPS points',
        missing: 'delivery addresses',
        source: 'routes',
        with: 'routes',
        optIn: true,
      },
    ],
  },
};
