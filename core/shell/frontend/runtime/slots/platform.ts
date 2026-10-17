import type { ComponentType } from 'react';
import type { LucideIcon } from 'lucide-react';
import type { AuditEvent } from '../../../../platform_owner/api/index.js';
import type { JobMetrics, PageReads } from '../../../../collection/api/index.js';
import type { PageFeature } from '../../../../tenancy/api/index.js';
import { manifests } from './installed.js';

// The platform owner's slots: what an owner puts in them loads only with the platform owner's
// pages, so no other page carries it.

/** A part of an audit log sentence: plain words, or words to stress. */
export type AuditPart = string | { strong: string };
/** What the audit log writes its sentences with. */
export type AuditWords = {
  strong: (value: string) => AuditPart;
  /** A `YYYY-MM-DD` day as the log reads it: "Sep 3". */
  day: (value: string) => string;
  /** A `HH:MM` time of day as the log reads it: "10:01 AM". */
  clock: (value: string) => string;
};
/** Each event's sentence after the actor's name, by action. */
export type AuditPhrases = Record<string, (event: AuditEvent, words: AuditWords) => AuditPart[]>;
/** How an owner's events read in the platform owner's audit log. */
export type AuditWording = {
  phrases?: AuditPhrases;
  /** The actions whose sentence already says what the event's detail holds. */
  spoken?: readonly string[];
  /** The names of the fields its events change. */
  fields?: Record<string, string>;
  /** How a value of one of its fields reads; undefined leaves it to the log. */
  value?: (field: string, value: string, words: AuditWords) => string | undefined;
  /** What its events add to their second line, after the log's own notes. */
  notes?: (event: AuditEvent) => string[];
  /** How a collection's outcome names it, as "<name> collection", by the connection that ran it. */
  collected?: Record<string, string>;
};

/**
 * How Diagnostics words the measurements core keeps of every collection's runs, beyond their
 * timings and memory: what an attempt collected, and its page reads.
 */
export type RunWording = {
  /** What an attempt collected; undefined says nothing, and the row reads "—". */
  collected: (attempt: JobMetrics) => string | undefined;
  /** The rows its page reads add to the attempt's measurements. */
  reads: (reads: PageReads) => [label: string, value: string][];
  /** Draws its slow and failed reads, if it has any, below them. */
  slowReads: ComponentType<{ attempt: JobMetrics }>;
};

/**
 * What an owner puts in the platform owner's slots. Only the platform owner's pages read them,
 * so those pages load every owner's with them and no other page carries them. Their labels,
 * such as the read toggles' and the collections', are generated from the backend instead.
 */
export type PlatformSlots = {
  /** Its page's switch, as the DSPs page lists it. */
  switch?: { id: PageFeature; icon: LucideIcon };
  /** How its events read in the audit log. */
  auditWording?: AuditWording;
  /** How Diagnostics words every collection's runs; the first owner listed with it does. */
  runWording?: RunWording;
  /** How the mail log names the kinds of email it sends a member, by kind. */
  mailKinds?: Record<string, string>;
};

declare module './installed.js' {
  interface FrontendFeature {
    /** Loads its module that exports what it puts in the platform owner's slots, as `slots`. */
    platformSlots?: () => Promise<{ slots: PlatformSlots }>;
  }
}

let loaded: { of: readonly unknown[]; slots: readonly PlatformSlots[] } | undefined;
let slotsLoad: { of: readonly unknown[]; load: Promise<void> } | undefined;
/**
 * Loads what every owner puts in the platform owner's slots, once for the manifests installed;
 * a failed load is tried again next time. The readers below find nothing until it has loaded.
 */
export function loadPlatformSlots() {
  const of = manifests();
  if (slotsLoad?.of !== of) {
    const load = Promise.all(
      of.flatMap(({ platformSlots }) =>
        platformSlots ? [platformSlots().then(({ slots }) => slots)] : [],
      ),
    ).then(
      (slots) => void (loaded = { of, slots }),
      (error: unknown) => {
        slotsLoad = undefined;
        throw error;
      },
    );
    slotsLoad = { of, load };
  }
  return slotsLoad.load;
}
/** What has loaded for the manifests installed, if it has. */
const loadedSlots = () => (loaded?.of === manifests() ? loaded.slots : []);

/** How the mail log names an owner's kind of email, once the slots have loaded. */
export const mailKindLabel = (kind: string) =>
  loadedSlots().find((slots) => slots.mailKinds?.[kind])?.mailKinds?.[kind];
/** The icon of a feature's switch: the one its platform slots name, else its page's. */
export const switchIcon = (id: string) =>
  loadedSlots().find((slots) => slots.switch?.id === id)?.switch?.icon ??
  manifests()
    .find((feature) => feature.name === id)
    ?.routes?.find((route) => route.icon)?.icon;

/** Every owner's audit wording, in the order the owners are listed. */
export const auditWording = () => loadedSlots().flatMap((slots) => slots.auditWording ?? []);

/** How Diagnostics words every collection's runs, as the first owner listed with it does. */
export const runWording = () => loadedSlots().find((slots) => slots.runWording)?.runWording;
