import type { ComponentType, ReactNode } from 'react';
import type { DspView, SessionView } from '../../../../accounts/api/index.js';
import type { Feature } from '../../../../tenancy/api/index.js';
import { manifests, present } from './installed.js';

// The Settings slots: an owner's tabs on a DSP's Settings page, and its pieces of other owners'
// tabs there.

/** What a Settings panel is drawn for: the person, and inside a DSP its view. */
export type SettingsContext = { session: SessionView; view?: DspView };
/** A tab of a Settings page. */
export type SettingsTab = {
  /** The tab's address, `?tab=<id>`. It never changes, so links to it keep working. */
  id: string;
  label: string;
  /**
   * Draws the label with more beside it, such as a count. Settings loads it with the page, as
   * part of the page's own code once was, so the tab never shows without it.
   */
  badge?: { load: () => Promise<ComponentType<{ active: boolean }>> };
  /** Where the tab sits among the page's tabs, lowest first. */
  order: number;
  /** Who sees the tab; omitted means everyone. */
  visible?: (view?: DspView) => boolean;
  /** The part of its owner's feature it belongs to, as `x.uploads`: it goes with the part. */
  part?: Feature;
  /** Loads the panel's code. */
  load: () => Promise<unknown>;
  render: (context: SettingsContext) => ReactNode;
  /**
   * The reads to warm, for the admitted view, while `tab` is about to open. A tab may name its
   * badge's reads whichever tab that is.
   */
  prefetch?: (tab: string, view: DspView) => string[];
};
/** A piece one owner adds to a Settings tab, drawn under the tab's own panel. */
export type SettingsPiece = {
  /** The tab it is drawn on. */
  tab: string;
  /** Its own id among the tab's pieces. */
  id: string;
  /** Where it sits among the tab's pieces, lowest first. */
  order: number;
  /** Drawn above the tab's own panel rather than under it. */
  above?: boolean;
  /** Who sees it; omitted means everyone who sees the tab. */
  visible?: (view?: DspView) => boolean;
  /** The part of its owner's feature it belongs to: it goes with the part. */
  part?: Feature;
  /** Loads its code; the tab waits for it before it opens. */
  load: () => Promise<unknown>;
  render: (context: SettingsContext) => ReactNode;
};

declare module './installed.js' {
  interface FrontendFeature {
    /** Its tabs on a DSP's Settings page. */
    settingsTabs?: readonly SettingsTab[];
    /** Its pieces of other owners' tabs on a DSP's Settings page. */
    settingsPieces?: readonly SettingsPiece[];
  }
}

/**
 * Every owner's tabs on a DSP's Settings page, in the order the owners are listed; with a view,
 * those it has.
 */
export const settingsTabs = (view?: DspView) =>
  view
    ? present(view, (feature) => feature.settingsTabs)
    : manifests().flatMap((feature) => feature.settingsTabs ?? []);
/** The pieces of a Settings tab the view has and sees, in their order. */
export const settingsPieces = (tab: string, view: DspView | undefined) =>
  present(view, (feature) => feature.settingsPieces)
    .filter((piece) => piece.tab === tab && (!piece.visible || piece.visible(view)))
    .sort((a, b) => a.order - b.order);
