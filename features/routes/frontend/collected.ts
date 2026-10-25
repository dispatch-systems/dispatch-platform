import {
  Box,
  Clock,
  Coffee,
  Hash,
  ListChecks,
  MapPin,
  Navigation,
  Package,
  Phone,
  Route,
  ScanBarcode,
  Smartphone,
  Tag,
  Timer,
  Truck,
  UserRound,
} from 'lucide-react';
import type { Collected } from '../../../core/shell/frontend/runtime/slots.js';

// What Routes collects, as Settings' Collectors page shows it: a day's routes, their itineraries,
// packages and the customers they're for. Each value is made up, to show what a field holds.

export const collected: Record<string, Collected> = {
  routes: {
    from: ['Operations', 'Execution', 'Routes'],
    records: [
      {
        label: 'Routes',
        icon: Route,
        fields: [
          { label: 'Route code', icon: Route, example: 'CX214' },
          { label: 'Service type', icon: Tag, example: 'Standard Parcel' },
          { label: 'Status', icon: ListChecks, example: 'Complete' },
          { label: 'Planned departure', icon: Clock, example: '10:20 AM' },
          { label: 'Stops', icon: MapPin, example: '152' },
          { label: 'Unassigned', icon: Package, example: 'None' },
        ],
      },
      {
        label: 'Itineraries',
        icon: UserRound,
        counted: 'itineraries',
        fields: [
          { label: 'Driver', icon: UserRound, example: 'Marcus Reyes' },
          { label: 'Work phone', icon: Smartphone, example: '(323) 555-0182' },
          { label: 'Van VIN', icon: Truck, example: '1FTBW3XM6NKA04821' },
          { label: 'Departure, finish', icon: Clock, example: '10:31 AM · 7:48 PM' },
          { label: 'Stops done', icon: MapPin, example: '148 of 152' },
          { label: 'Packages', icon: Package, example: '231 delivered · 3 left' },
          { label: 'Breaks', icon: Coffee, example: '30 min' },
          { label: 'Overtime', icon: Timer, example: 'None' },
        ],
      },
      {
        label: 'Packages',
        icon: Package,
        fields: [
          { label: 'Tracking ID', icon: ScanBarcode, example: 'TBA312049981204' },
          { label: 'Order ID', icon: Hash, example: '113-4820193-2209184' },
          { label: 'Delivery window', icon: Clock, example: '10:00 AM – 2:00 PM' },
          { label: 'Box size and weight', icon: Box, example: 'Medium · 2.4 lb' },
        ],
      },
      {
        label: 'Customers',
        icon: MapPin,
        fields: [
          { label: 'Name', icon: UserRound, example: 'J. Alvarez', personal: 'Customer name' },
          { label: 'Phone', icon: Phone, example: '(323) 555-0147', personal: 'Customer phone' },
          { label: 'Address', icon: MapPin, example: 'Los Angeles, CA 90017', personal: 'Address' },
          {
            label: 'Location',
            icon: Navigation,
            example: '34.0522, -118.2437',
            personal: 'Delivery location',
          },
        ],
      },
    ],
    period: [
      { label: 'Yesterday', detail: 'And up to 4 missed days before it' },
      { label: 'Today so far' },
    ],
    each: 'Replaces the day',
  },
};
