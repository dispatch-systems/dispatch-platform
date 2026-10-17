import type { ReactNode } from 'react';
import type { LucideIcon } from 'lucide-react';
import type { DspView } from '../../../../accounts/api/index.js';
import type { ConnectionFeature, Feature } from '../../../../tenancy/api/index.js';
import { manifests, present } from './installed.js';
import type { SettingsContext } from './settings.js';

// The Connections slots: a collector's card on a DSP's Connections tab, the cards features add
// beside it, and the sections an owner adds to a connection's settings.

/**
 * A card a feature adds to a DSP's Connections tab, beside the collectors' own: one of the DSP's
 * accounts, which those with Manage DSP Connections manage, or the member's own account.
 */
export type ConnectionPiece = {
  /** Its own id among the section's cards. */
  id: string;
  /** `dsp` for one of the DSP's accounts, `personal` for the member's own. */
  section: 'dsp' | 'personal';
  /** Where it sits among the section's cards, after the collectors', lowest first. */
  order: number;
  /** Who sees it; omitted means everyone who sees the section. */
  visible?: (view?: DspView) => boolean;
  /**
   * Who else sees it in the `dsp` section, beyond those who manage the DSP's connections, as
   * those who hold a permission of its own; the card shows each only what they may do.
   */
  others?: (view?: DspView) => boolean;
  /** The part of its owner's feature it belongs to: it goes with the part. */
  part?: Feature;
  /** The reads it shows, warmed while the Connections tab is about to open. */
  reads: readonly string[];
  /** Loads its code; the tab loads every card's before it opens. */
  load: () => Promise<unknown>;
  /** Its cards: as many as the DSP has of its accounts, or none. */
  render: (context: SettingsContext) => ReactNode;
  /** Its entry in New on DSP Connections, for an account the DSP may have several of. */
  adds?: NewConnection;
};
/** A kind of account New on DSP Connections adds, for those who manage the DSP's connections. */
export type NewConnection = {
  label: string;
  /** Its mark beside the label, drawn from its card's code. */
  icon: () => ReactNode;
  /** What choosing it opens, which calls `close` once it's done or dismissed. */
  render: (context: SettingsContext & { close: () => void }) => ReactNode;
};

/**
 * A section one owner adds to the settings of a connection, which the connection's owner draws
 * among its own, one at a time: a feature's settings for each of the DSP's Slack workspaces, say.
 */
export type ConnectionSection = {
  /** The connection whose settings it's a section of, as that connection's owner names it. */
  connection: string;
  /** Its own id among the connection's sections. */
  id: string;
  label: string;
  /** Its mark beside the label where the sections are listed. */
  icon?: LucideIcon;
  /** Where it sits among the connection's sections, lowest first. */
  order: number;
  /** Who sees it; omitted means everyone who sees the connection's settings. */
  visible?: (view?: DspView) => boolean;
  /** The part of its owner's feature it belongs to: it goes with the part. */
  part?: Feature;
  /**
   * The integration whose settings it is, by its id: it's listed under the connection's
   * integrations, and only while that's on for the account.
   */
  integration?: string;
  /** Draws it for the connection's account `account`, as its owner names that. */
  render: (context: ConnectionSectionContext) => ReactNode;
};
/** What a section of a connection's settings is drawn with. */
export type ConnectionSectionContext = SettingsContext & {
  /** The connection's account it's drawn for, as the connection's owner names it. */
  account: string;
  /** Opens another of the connection's sections, by its id. */
  open: (section: string) => void;
};

/** What a connection's card is drawn with on a DSP's Connections page. */
export type ConnectionCardContext = { development: boolean; timezone: string };
/** A collector's card on a DSP's Connections page. */
export type ConnectionCard = {
  /** The connection's catalog id. */
  provider: ConnectionFeature;
  /** The read the card shows, warmed while the Connections tab is about to open. */
  read: string;
  /** Loads the card's code. */
  load: () => Promise<unknown>;
  render: (context: ConnectionCardContext) => ReactNode;
};

declare module './installed.js' {
  interface FrontendFeature {
    /** Its connection's card. */
    connectionCard?: ConnectionCard;
    /** Its cards on a DSP's Connections tab. */
    connectionPieces?: readonly ConnectionPiece[];
    /** Its sections of a connection's settings, its own or another owner's. */
    connectionSections?: readonly ConnectionSection[];
  }
}

/** Every collector's connection card, in the order the collectors are listed. */
export const connectionCards = () =>
  manifests().flatMap((feature) => (feature.connectionCard ? [feature.connectionCard] : []));
/** The card of a connection. */
export const connectionCard = (provider: string) =>
  connectionCards().find((card) => card.provider === provider);
/**
 * The cards features add to a section of the Connections tab: with a view, those it has and
 * sees, in their order; without one, every one, for loading their code.
 */
export const connectionPieces = (section: ConnectionPiece['section'], view?: DspView) =>
  (view
    ? present(view, (feature) => feature.connectionPieces).filter(
        (piece) => !piece.visible || piece.visible(view),
      )
    : manifests().flatMap((feature) => feature.connectionPieces ?? [])
  )
    .filter((piece) => piece.section === section)
    .sort((a, b) => a.order - b.order);
/** The sections of the settings of `connection` the view has and sees, in their order. */
export const connectionSections = (connection: string, view: DspView) =>
  present(view, (feature) => feature.connectionSections)
    .filter((section) => section.connection === connection)
    .filter((section) => !section.visible || section.visible(view))
    .sort((a, b) => a.order - b.order);
