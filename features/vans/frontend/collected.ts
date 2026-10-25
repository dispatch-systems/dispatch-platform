import { Box, Hash, ListChecks, MapPin, Tag, Truck } from 'lucide-react';
import type { Collected } from '../../../core/shell/frontend/runtime/slots.js';

// What Vans collects, as Settings' Collectors page shows it: every vehicle Amazon's fleet
// management lists for the DSP. Each value is made up, to show what a field holds.

export const collected: Record<string, Collected> = {
  vans: {
    from: ['Fleet Management', 'Vehicles'],
    records: [
      {
        label: 'Vans',
        icon: Truck,
        counted: 'rows',
        fields: [
          { label: 'Name', icon: Tag, example: 'CDV 101' },
          { label: 'VIN', icon: Truck, example: '1FTBW3XM6NKA04821' },
          { label: 'License plate', icon: Hash, example: '8XYZ214' },
          { label: 'Status', icon: ListChecks, example: 'Active' },
          { label: 'Van type', icon: Box, example: 'Extra large cargo van' },
          { label: 'Station', icon: MapPin, example: 'DLA7' },
        ],
      },
    ],
    period: [{ label: 'Whole fleet', detail: 'Every status' }],
    each: 'Replaces the fleet, keeping vans no longer listed',
  },
};
