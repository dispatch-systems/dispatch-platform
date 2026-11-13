import type { ComponentType } from 'react';
import type { LucideIcon } from 'lucide-react';
import type { DspView } from '../../../../accounts/api/index.js';
import type { Feature } from '../../../../tenancy/api/index.js';
import { present } from './installed.js';
import type { Access } from './pages.js';

// The widgets slot: what each feature shows of itself on a member's Home page. A widget sits where
// the member places it on the page's grid, at one of the sizes it declares, which resizing it
// snaps between, and fits what it shows to that size: it is drawn with its columns and rows, and
// its own CSS fits within them.

/**
 * A widget's size in the Home page grid's square cells: of twelve columns by seven rows, as large
 * as the window allows, so a layout is the same on every screen and the page never scrolls.
 */
export type WidgetCells = { columns: number; rows: number };

/** What a widget is drawn with: its size in cells, and the view of the DSP open. */
export type WidgetProps = WidgetCells & { view: DspView };

/** One of an owner's widgets. */
export type Widget = {
  /** Permanent once shipped, as members' Home pages keep it: `<feature>.<what>`. */
  id: string;
  /** Its name in the list of widgets to add. */
  label: string;
  icon: LucideIcon;
  /**
   * The sizes it draws, in cells, each once and from one cell to the whole grid: the first the
   * one it is added at, and resizing it snaps to the nearest of them.
   */
  sizes: readonly [WidgetCells, ...WidgetCells[]];
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
