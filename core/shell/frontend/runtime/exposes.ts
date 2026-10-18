import type { DspView, Permission } from '../../../accounts/api/index.js';
import type { Feature } from '../../../tenancy/api/index.js';
import { exposes } from '../../../manifest/api/generated/exposes.js';
import { hasFeature } from './features.js';

// What features expose each other (`core/manifest/backend/exposes.rs`) that a page shows, as a
// page using one sees it: whether the DSP has it, what it's called, and who sees what it keeps.
// A page never names the feature offering one.

/** What's exposed that a page shows, by name. */
export type ExposedName = 'files';
/** One as a page using it sees it. */
export type Offered = {
  /** The feature offering it, by its switch. */
  feature: string;
  /** What it's called where members see it, as its page is named. */
  name: string;
  /** The permission that shows what it keeps. */
  permission: Permission;
};

const all: Partial<Record<ExposedName, Offered>> = exposes;

/** What's exposed as `name` that the DSP's view has: only while the feature offering it is on there. */
export function offered(name: ExposedName, view: DspView | undefined): Offered | undefined {
  const exposed = all[name];
  return exposed && hasFeature(view, exposed.feature as Feature) ? exposed : undefined;
}
