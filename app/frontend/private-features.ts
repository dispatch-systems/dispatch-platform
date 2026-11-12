import type { DspView, SessionView } from '../../core/accounts/api/index.js';

type Modules = {
  custom: typeof import('./custom-features.js');
  platform: typeof import('./platform-features.js');
};
let loaded: Modules | undefined;
let reading: Promise<Modules> | undefined;
let customGeneration = 0;
let platformGeneration = 0;
let platformStyles = false;

// Ordinary DSPs need none of the private admission machinery. Keep its synchronous
// boundary hooks once loaded, including while a feature's own import is still pending.
function load() {
  return (reading ??= Promise.all([
    import('./custom-features.js'),
    import('./platform-features.js'),
  ])
    .then(([custom, platform]) => {
      loaded = { custom, platform };
      custom.setPlatformCustomStyles(platformStyles);
      return loaded;
    })
    .catch((error: unknown) => {
      reading = undefined;
      throw error;
    }));
}
export function clearCustomFeatures() {
  customGeneration++;
  loaded?.custom.clearCustomFeatures();
}
export function clearPlatformFeatures() {
  platformGeneration++;
  platformStyles = false;
  loaded?.platform.clearPlatformFeatures();
}
export function setPlatformCustomStyles(active: boolean) {
  platformStyles = active;
  loaded?.custom.setPlatformCustomStyles(active);
}
export async function prepareCustomFeatures(view: DspView, current: () => boolean) {
  if (!view.custom) return;
  const generation = customGeneration;
  const { custom } = await load();
  const admitted = () => generation === customGeneration && current();
  if (admitted()) return custom.prepareCustomFeatures(view, admitted);
}
export async function preparePlatformFeatures(session: SessionView, current: () => boolean) {
  if (!session.platformFrontend) return;
  const generation = platformGeneration;
  const { platform } = await load();
  const admitted = () => generation === platformGeneration && current();
  if (admitted()) return platform.preparePlatformFeatures(session, admitted);
}
