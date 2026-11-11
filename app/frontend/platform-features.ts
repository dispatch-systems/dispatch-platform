import type { SessionView } from '../../core/accounts/api/index.js';
import {
  installCustomPlatformFeatures,
  type FrontendFeature,
} from '../../core/shell/frontend/runtime/slots.js';

type Loaded = {
  features: readonly FrontendFeature[];
  featureLabels: Readonly<Record<string, string>>;
  permissionLabels: Readonly<Record<string, string>>;
};
let generation = 0;

export function clearPlatformFeatures() {
  generation++;
  installCustomPlatformFeatures();
}

/** The server supplies this private entry only to verified platform owners. */
export async function preparePlatformFeatures(session: SessionView, current: () => boolean) {
  if (
    !session.user.platformOwner ||
    (session.security.required && !session.security.verified) ||
    !session.platformFrontend
  )
    return;
  const request = generation;
  const loaded = (await import(/* @vite-ignore */ session.platformFrontend)) as Loaded;
  if (!current() || request !== generation) return;
  return () =>
    installCustomPlatformFeatures(loaded.features, {
      features: loaded.featureLabels,
      permissions: loaded.permissionLabels,
    });
}
