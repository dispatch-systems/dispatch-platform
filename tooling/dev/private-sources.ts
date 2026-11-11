import path from 'node:path';
import fs from 'node:fs';
import type { IncomingMessage } from 'node:http';
import type { Plugin } from 'vite';
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
export function privateSources(directory: string) {
  const root = fs.realpathSync(directory);
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
    let file = route.startsWith('/@fs/')
      ? path.resolve(route.slice(4))
      : path.resolve(root, 'app/frontend', route.slice(1));
    try {
      file = fs.realpathSync(file);
    } catch {
      // Virtual modules and missing paths have no filesystem target.
    }
    const owner = /\/features\/custom\/([a-z0-9]+)\//.exec(file)?.[1];
    if (owner) return { owner };
    // A later middleware must not turn an undecoded private path into a public one.
    if (route.includes('%')) return { platform: true };
    return inventories.has(path.relative(root, file).replaceAll('\\', '/'))
      ? { platform: true }
      : undefined;
  };
}

/** npm run dev protects its outer server and preview cookies. Standalone Vite has no
 * outer server, so it must protect the same source requests with its normal session. */
export function privateSourceGuard(root: string, backend: string): Plugin {
  const audience = privateSources(root);
  return {
    name: 'dispatch-private-sources',
    configureServer(server) {
      if (server.config.server.middlewareMode) return;
      server.middlewares.use((request, response, next) => {
        const owner = audience(request.url ?? '/');
        if (!owner) return next();
        void mayReadSource(backend, request, request.headers.cookie, owner).then((allowed) => {
          if (allowed) next();
          else {
            response.writeHead(404, { 'cache-control': 'private, no-store' });
            response.end();
          }
        });
      });
    },
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
