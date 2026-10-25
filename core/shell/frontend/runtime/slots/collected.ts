import type { LucideIcon } from 'lucide-react';
import type { RunCounts } from '../../../../collection/api/index.js';
import { manifests } from './installed.js';

// The Collectors slot: what each collection a feature keeps collects, which Settings' Collectors
// page shows under the collection chosen.

/** One thing each record holds, with a made-up value of the kind it holds. */
export type CollectedField = {
  label: string;
  icon: LucideIcon;
  example: string;
  /**
   * Whether it's someone's own: their contact, place or likeness. Named so among the personal
   * fields, or by `label` when it's `true`.
   */
  personal?: true | string;
};
/** One kind of record a collection keeps, as Employees or Timecards. */
export type CollectedRecords = {
  label: string;
  icon: LucideIcon;
  /** The count of the latest successful run that counts them. */
  counted?: keyof RunCounts;
  fields: readonly CollectedField[];
};
/** What a run reads, as "Current pay period", with what more it says, as "Every employee". */
export type CollectedPeriod = { label: string; detail?: string };
/** What a collection collects. */
export type Collected = {
  /** Where its collector reads it, under the collector's name: "Time & Attendance", "Timecard Search". */
  from: readonly string[];
  /** Whether its records are the source's datasets, kept as read, rather than fields of its own. */
  datasets?: boolean;
  records: readonly CollectedRecords[];
  /** What a run reads; for a collection whose rules choose what they read, each choice's. */
  period: readonly CollectedPeriod[];
  /** What each run does with what's kept: "Replaces the pay period". */
  each: string;
  /** What a run leaves unread. */
  skips?: readonly string[];
  /** What it reads but doesn't keep. */
  dropped?: readonly string[];
};

declare module './installed.js' {
  interface FrontendFeature {
    /**
     * Loads its module that exports what each collection it keeps collects, as `collected`, by
     * the collection's name on the Collectors page.
     */
    collected?: () => Promise<{ collected: Readonly<Record<string, Collected>> }>;
  }
}

/** What a collection collects, as `feature`, the feature that keeps it, says; if it says. */
export async function collectedOf(feature: string, collection: string) {
  const load = manifests().find((each) => each.name === feature)?.collected;
  return load && (await load()).collected[collection];
}
