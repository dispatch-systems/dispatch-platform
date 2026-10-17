import type { CollectionChange } from '../../../../collection/api/index.js';
import type { Replies } from '../../../../foundation/api/runtime.js';
import { first, manifests } from './installed.js';

// The API slots: how the response cache treats an owner's reads, which of them wait for a
// change, how its routes' replies are checked, and what its error codes say.

/** How the response cache keeps an owner's reads current, each read named by its path prefix. */
export type CacheRules = {
  /** Its reads that hold collected data. */
  collected?: readonly string[];
  /** Whether a finished collection's changes reach one of its `collected` reads; omitted, any do. */
  collection?: (url: string, changes: readonly CollectionChange[]) => boolean;
  /** Its reads that change as a job starts, runs or ends; a write under one starts or ends one. */
  jobs?: readonly string[];
  /** Its reads that change with the DSP's connections. */
  connections?: readonly string[];
  /**
   * Whether a write changes a read, for the writes it knows; undefined for the others. A read
   * changes when any owner says so, and a write an owner knows changes nothing else.
   */
  write?: (write: string, url: string) => boolean | undefined;
};

declare module './installed.js' {
  interface FrontendFeature {
    /** How the response cache treats its reads. */
    cache?: CacheRules;
    /** Its reads that wait for a change before they answer, by path prefix. */
    longPolls?: readonly string[];
    /**
     * Loads its module that exports how the API client checks the replies of its routes, as
     * `replies`, as core checks its own. It loads apart from the first page, as its pages do.
     */
    replies?: () => Promise<{ replies: Replies }>;
    /** What its error codes say. */
    errors?: Record<string, string>;
    /** Why a schedule of its collections waits, by the code it last stopped on; its error too. */
    scheduleIssues?: Record<string, string>;
  }
}

/** Every owner's cache rules, in the order the owners are listed. */
export const cacheRules = () => manifests().flatMap((feature) => feature.cache ?? []);

/** Whether a read waits for a change before it answers. */
export const isLongPoll = (url: string) =>
  manifests().some((feature) => feature.longPolls?.some((prefix) => url.startsWith(prefix)));

/** What an owner's error code says. */
export const errorLabelOf = (code: string) =>
  first((feature) => feature.errors?.[code] ?? feature.scheduleIssues?.[code]);
/** Why a schedule of an owner's collections waits. */
export const scheduleIssueOf = (code: string) => first((feature) => feature.scheduleIssues?.[code]);

/** Every owner's reply validators, loaded once for the manifests installed. */
let repliesLoad: { of: readonly unknown[]; load: Promise<Replies[]> } | undefined;
/**
 * Loads how every owner checks its routes' replies, once; a failed load is tried again next
 * time.
 */
export function loadReplyChecks() {
  const of = manifests();
  if (repliesLoad?.of !== of) {
    const load = Promise.all(
      of.flatMap(({ replies }) => (replies ? [replies().then((module) => module.replies)] : [])),
    ).catch((error: unknown) => {
      repliesLoad = undefined;
      throw error;
    });
    repliesLoad = { of, load };
  }
  return repliesLoad.load;
}
