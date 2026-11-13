import type { ComponentType } from 'react';
import type { LucideIcon } from 'lucide-react';
import type { DspView } from '../../../../accounts/api/index.js';
import type { Feature } from '../../../../tenancy/api/index.js';
import { present } from './installed.js';
import type { Access } from './pages.js';

// The widgets slot: what each feature shows of itself on a member's Home page. A widget sits where
// the member places it on the page's grid, at one of the size steps it supports, which resizing
// it snaps between, and draws what fits that step; within a step, its own CSS fits what it shows
// to the frame.

/**
 * The size steps a widget may be drawn at, in the Home page grid's cells: twelve columns across
 * the page and twelve rows down the window, which stretch with it, so a layout is the same on
 * every screen and the page never scrolls. Small is a sixteenth of the page, full half of it.
 */
export const widgetSizes = {
  small: { columns: 3, rows: 3 },
  wide: { columns: 6, rows: 3 },
  tall: { columns: 3, rows: 6 },
  large: { columns: 6, rows: 6 },
  full: { columns: 12, rows: 6 },
} as const;
export type WidgetSize = keyof typeof widgetSizes;

/** What a widget is drawn with: the step it's at, and the view of the DSP open. */
export type WidgetProps = { size: WidgetSize; view: DspView };

/** One of an owner's widgets. */
export type Widget = {
  /** Permanent once shipped, as members' Home pages keep it: `<feature>.<what>`. */
  id: string;
  /** Its name in the list of widgets to add. */
  label: string;
  icon: LucideIcon;
  /** The steps it draws, each once, the first the one it is added at. */
  sizes: readonly [WidgetSize, ...WidgetSize[]];
  /** Who may add it and see it; omitted means every member. */
  permission?: (access: Access) => boolean;
  /** The part of its owner's feature it goes with: off or hidden with it. */
  part?: Feature;
  /** The ids it once had, which a Home page that kept one shows it for. */
  formerly?: readonly string[];
  /** Its place on the Home page of a member who never arranged theirs, lowest first; omitted, it isn't on it. */
  home?: number;
  /** Loads what draws it. */
  load: () => Promise<ComponentType<WidgetProps>>;
};

declare module './installed.js' {
  interface FrontendFeature {
    /** Its widgets for each member's Home page. */
    widgets?: readonly Widget[];
  }
}

/** The widgets the member may add in the DSP they have open, in the order the owners are listed. */
export const widgets = (access: Access & { view: DspView }) =>
  present(access.view, (feature) => feature.widgets).filter(
    (widget) => !widget.permission || widget.permission(access),
  );
