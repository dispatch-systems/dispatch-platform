import {
  Building2,
  CalendarSync,
  Database,
  Palette,
  Plug,
  Settings2,
  ShieldCheck,
  SlidersHorizontal,
  User,
  type LucideIcon,
} from 'lucide-react';
import type { SettingsGroup } from '../../../../core/settings/api/index.js';
import { featureIcon } from '../../../../core/shell/frontend/runtime/slots.js';
import type { ListedPage } from '../pages.js';

// How the sidebar shows Settings' pages: under their group's name, each with its icon.

/** Each group's name over its pages. */
export const groupLabels: Record<SettingsGroup, string> = {
  personal: 'Personal',
  dsp: 'DSP',
  features: 'Features',
};

/** The icons of core's shared pages, by their ids. */
const shared: Record<string, LucideIcon> = {
  general: User,
  security: ShieldCheck,
  theme: Palette,
  display: SlidersHorizontal,
  dsp: Building2,
  connections: Plug,
  collectors: CalendarSync,
  data: Database,
};

/**
 * A page's icon: a shared page's own, a feature's page its feature's, as the feature's own pages
 * show it (its switch's id is the page's, `_` for `-`), and a page a feature puts under Settings
 * its own; else a plain one.
 */
export const pageIcon = (page: ListedPage): LucideIcon =>
  (page.route
    ? page.route.icon
    : page.group === 'features'
      ? featureIcon(page.id.replaceAll('-', '_'))
      : shared[page.id]) ?? Settings2;
