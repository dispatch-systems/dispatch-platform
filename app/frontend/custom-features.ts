import type { DspView } from '../../core/accounts/api/index.js';

let generation = 0;
let loaded: typeof import('./custom-feature-runtime.js') | undefined;

/** A workspace boundary invalidates loading code before it can install private UI. */
export function clearCustomFeatures() {
  generation++;
  loaded?.clearCustomFeatures();
}

/** DSPs without custom metadata need none of the private module and stylesheet loader. */
export async function prepareCustomFeatures(view: DspView, current: () => boolean) {
  const custom = view.custom;
  if (!custom?.catalog.length && !custom?.permissions.length && !custom?.frontends.length) return;
  const request = generation;
  const runtime = await import('./custom-feature-runtime.js');
  if (!current() || request !== generation) return;
  loaded = runtime;
  return runtime.prepareCustomFeatures(view, () => current() && request === generation);
}
