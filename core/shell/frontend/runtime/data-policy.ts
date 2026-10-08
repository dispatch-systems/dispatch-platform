/** Cache dependencies, independent of React and transport: core's own, and each owner's rules. */
import type { CollectionChange } from '../../../collection/api/index.js';
import { cacheRules } from './slots.js';
export type { CollectionChange };
export const path = (url: string) => url.split('?')[0]!;
export const begins = (url: string, ...prefixes: readonly string[]) =>
  prefixes.some((p) => path(url).startsWith(p));
/** The owner's rules for a read of its collected data. */
const collector = (url: string) =>
  cacheRules().find((rules) => begins(url, ...(rules.collected ?? [])));
export const collectionData = (url: string) => collector(url) !== undefined;

export function collectionAffects(url: string, changes: readonly CollectionChange[]) {
  const rules = collector(url);
  if (!rules) return false;
  return rules.collection?.(url, changes) ?? true;
}

export function mutationAffects(mutation: string, url: string) {
  const write = path(mutation);
  if (write === '/api/session/dsp' || write.startsWith('/api/auth/')) return false;
  const rules = cacheRules();
  const said = rules.map((owner) => owner.write?.(write, url));
  if (said.includes(true)) return true;
  if (said.includes(false)) return false;
  const jobs = rules.flatMap((owner) => owner.jobs ?? []);
  // A write under a read that changes with jobs starts or cancels one, as a sync or collect does.
  if (begins(write, ...jobs) || write.endsWith('/sync') || write.endsWith('/collect'))
    return begins(url, ...jobs) || (write.endsWith('/sync') && path(url) === write.slice(0, -5));
  if (write.startsWith('/api/dsp/connections'))
    return begins(
      url,
      '/api/dsp/connections',
      ...jobs,
      ...rules.flatMap((owner) => owner.connections ?? []),
    );
  if (write.startsWith('/api/platform/')) return url.startsWith('/api/platform/');
  // Unclassified DSP edits conservatively revalidate their DSP, not platform pages.
  return write.startsWith('/api/dsp/') && url.startsWith('/api/dsp/');
}
