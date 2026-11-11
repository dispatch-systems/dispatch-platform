import path from 'node:path';
import type { IncomingMessage } from 'node:http';
import { featureLocations } from '../shared/features.js';

type Audience = { owner: string } | { platform: true };
// These inventories contain several owners' identifiers. They are never browser inputs.
const inventories = new Set([
  'Cargo.toml',
  'app/backend/Cargo.toml',
  'app/backend/features.rs',
  'app/generated/features.json',
  'core/tenancy/api/generated/custom-types.ts',
  'core/tenancy/api/generated/audit-subjects.ts',
  'core/manifest/api/generated/PeopleKind.ts',
  'core/manifest/api/generated/PeopleSource.ts',
  'core/platform_owner/api/runtime.ts',
]);

/** Protect source requests as well as release chunks; Vite otherwise serves /@fs publicly. */
export function privateSources(root: string) {
  const owners = new Map(
    featureLocations(root)
      .filter((feature) => feature.owner)
      .map((feature) => [feature.name, feature.owner!]),
  );
  return (url: string): Audience | undefined => {
    let route: string;
    try {
      route = decodeURIComponent(url.split('?')[0]!).replaceAll('\\', '/');
    } catch {
      return { platform: true };
    }
    const entry = /^\/(?:custom\/([^/]+)\.js|@id\/__x00__dispatch-custom:([^/]+))$/.exec(route);
    if (entry) {
      const owner = owners.get(entry[1] ?? entry[2]!);
      return owner ? { owner } : { platform: true };
    }
    const file = route.startsWith('/@fs/')
      ? path.resolve(route.slice(4))
      : path.resolve(root, 'app/frontend', route.slice(1));
    const owner = /\/features\/custom\/([a-z0-9]+)\//.exec(file)?.[1];
    if (owner) return { owner };
    return inventories.has(path.relative(root, file).replaceAll('\\', '/'))
      ? { platform: true }
      : undefined;
  };
}

export async function mayReadSource(
  backend: string,
  request: IncomingMessage,
  cookie: string | undefined,
  audience: Audience,
): Promise<boolean> {
  if (!cookie || !request.headers.host) return false;
  try {
    const response = await fetch(`${backend}/api/session`, {
      headers: { host: request.headers.host, cookie },
      redirect: 'manual',
    });
    if (!response.ok) return false;
    const session = (await response.json()) as {
      user: { platformOwner: boolean };
      security: { required: boolean; verified: boolean };
      dsps: { code: string | null; status: string }[];
    };
    if (session.security.required && !session.security.verified) return false;
    return (
      session.user.platformOwner ||
      ('owner' in audience &&
        session.dsps.some((dsp) => dsp.code === audience.owner && dsp.status === 'active'))
    );
  } catch {
    return false;
  }
}
