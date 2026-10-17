import type { DspView, Permission } from '../../../accounts/api/index.js';
import type { Feature } from '../../../tenancy/api/index.js';
import { services } from '../../../manifest/api/generated/services.js';
import { hasFeature } from './features.js';

// The services features offer each other (`core/manifest/backend/services/`), as a page using
// one sees them: whether the DSP has it, what it's called, and who sees what it keeps. A page
// never names the feature offering one.

/** The services there are, by name. */
export type ServiceName = 'files';
/** A service as a page using it sees it. */
export type Offered = {
  /** The feature offering it, by its switch. */
  feature: string;
  /** What it's called where members see it, as its page is named. */
  name: string;
  /** The permission that shows what it keeps. */
  permission: Permission;
};

const all: Partial<Record<ServiceName, Offered>> = services;

/** The service `name` the DSP's view has: only while the feature offering it is on there. */
export function offered(name: ServiceName, view: DspView | undefined): Offered | undefined {
  const service = all[name];
  return service && hasFeature(view, service.feature as Feature) ? service : undefined;
}
