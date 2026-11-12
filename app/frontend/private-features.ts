import type { DspView, SessionView } from '../../core/accounts/api/index.js';

type Implementation = typeof import('./custom-features.js');
let loaded: Implementation | undefined;
let reading: Promise<Implementation> | undefined;
let platformStyles = false;

// Once private admission has loaded, workspace boundaries clear it synchronously even
// while a feature's own import is pending. The caller's admission guards staged loads.
function load() {
  return (reading ??= import('./custom-features.js')
    .then((implementation) => {
      loaded = implementation;
      implementation.setPlatformCustomStyles(platformStyles);
      return implementation;
    })
    .catch((error: unknown) => {
      reading = undefined;
      throw error;
    }));
}
export function clearCustomFeatures() {
  loaded?.clearCustomFeatures();
}
export function clearPlatformFeatures() {
  platformStyles = false;
  loaded?.clearPlatformFeatures();
}
export function setPlatformCustomStyles(active: boolean) {
  platformStyles = active;
  loaded?.setPlatformCustomStyles(active);
}
export async function prepareCustomFeatures(view: DspView, current: () => boolean) {
  if (!view.custom) return;
  const implementation = await load();
  if (current()) return implementation.prepareCustomFeatures(view, current);
}
export async function preparePlatformFeatures(session: SessionView, current: () => boolean) {
  if (!session.platformFrontend) return;
  const implementation = await load();
  if (current()) return implementation.preparePlatformFeatures(session, current);
}
