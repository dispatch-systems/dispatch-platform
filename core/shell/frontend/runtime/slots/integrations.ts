import type { ReactNode } from 'react';
import type { LucideIcon } from 'lucide-react';
import type { DspView } from '../../../../accounts/api/index.js';
import type { Feature } from '../../../../tenancy/api/index.js';
import { featureCatalog } from '../features.js';
import { presentBy } from './installed.js';
import type { SettingsContext } from './settings.js';

// The integrations slot: what a feature does through another owner's connection, switched on
// and off in that connection's settings for each of its accounts.

/**
 * Something a feature does through another's connection, which the connection's settings switch
 * on and off for each of its accounts, as Documents saves the files posted in Slack. Its backend
 * declares it under the same id, with what it needs and hears of. Here it says what it is, for
 * members who don't know: what it does, how it's used and what it sees, on its card, and what
 * switching it on or off means, before it's switched.
 */
export type ConnectionIntegration = {
  /** The connection it works through, as that connection's owner names it. */
  connection: string;
  /** As its backend declares it: `<feature>.<what>`. */
  id: string;
  label: string;
  icon: LucideIcon;
  /** What it does, in a sentence or two. */
  description: string;
  /**
   * The line under its name, for the view: where it saves, say. Without one, the card names the
   * feature it's part of.
   */
  byline?: (view: DspView) => string | undefined;
  /** How it's used, step by step. */
  steps: readonly string[];
  /** What Dispatch sees through the connection while it's on. */
  sees: readonly IntegrationNote[];
  /** What switching it on means, said before it's switched on. */
  turningOn: readonly IntegrationNote[];
  /** What switching it off means, said before it's switched off. */
  turningOff: readonly IntegrationNote[];
  /** Where it sits among the connection's integrations, lowest first. */
  order: number;
  /** Who sees it; omitted means everyone who sees the connection's settings. */
  visible?: (view?: DspView) => boolean;
  /** The part of its owner's feature it belongs to: it goes with the part. */
  part?: Feature;
  /** What it's doing in the account, on its card while it's on. */
  summary?: (context: SettingsContext & { account: string }) => ReactNode;
};
/** One thing an integration says of itself, with its mark; without one, a tick. */
export type IntegrationNote = { text: string; icon?: LucideIcon };
/** An integration as the connection's settings list it, with the name of its feature, `from`. */
export type ListedIntegration = ConnectionIntegration & { from: string };

declare module './installed.js' {
  interface FrontendFeature {
    /** What it does through other owners' connections, switched on in their settings. */
    connectionIntegrations?: readonly ConnectionIntegration[];
  }
}

/**
 * The integrations through `connection` the view has and sees, in their order, each with the
 * name of the feature it's part of, `from`.
 */
export const connectionIntegrations = (connection: string, view: DspView): ListedIntegration[] =>
  presentBy(view, (feature) => feature.connectionIntegrations)
    .filter(({ each }) => each.connection === connection)
    .filter(({ each }) => !each.visible || each.visible(view))
    .map(({ owner, each }) => ({
      ...each,
      from: featureCatalog.find((f) => f.id === owner.name)?.label ?? owner.name,
    }))
    .sort((a, b) => a.order - b.order);
