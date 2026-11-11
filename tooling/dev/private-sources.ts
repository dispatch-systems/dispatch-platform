import path from 'node:path';
import fs from 'node:fs';
import http from 'node:http';
import type { IncomingMessage } from 'node:http';
import type { HotPayload, Plugin, ViteDevServer } from 'vite';
import { featureLocations } from '../shared/features.js';

type Audience = { owner: string } | { platform: true };
// These inventories contain several owners' identifiers. They are never browser inputs.
const inventories = new Set(
  [
    'Cargo.toml',
    'Cargo.lock',
    'app/backend/Cargo.toml',
    'app/backend/features.rs',
    'app/generated/features.json',
    'core/tenancy/api/generated/custom-types.ts',
    'core/tenancy/api/generated/audit-subjects.ts',
    'core/manifest/api/generated/PeopleKind.ts',
    'core/manifest/api/generated/PeopleSource.ts',
    'core/platform_owner/api/runtime.ts',
    'core/platform_owner/api/generated/custom-labels.ts',
  ].map((file) => file.toLowerCase()),
);

/** Protect source requests as well as release chunks; Vite otherwise serves /@fs publicly. */
export function privateSources(directory: string) {
  const root = fs.realpathSync(directory);
  const owners = new Map(
    featureLocations(root)
      .filter((feature) => feature.owner)
      .map((feature) => [feature.name, feature.owner!]),
  );
  return (url: string): Audience | undefined => {
    if (url.startsWith('\0dispatch-custom')) return { platform: true };
    let route: string;
    try {
      route = decodeURIComponent(new URL(url, 'http://vite.local').pathname).replaceAll('\\', '/');
    } catch {
      return { platform: true };
    }
    const entry = /^\/(?:custom\/([^/]+)\.js|@id\/__x00__dispatch-custom:([^/]+))$/.exec(route);
    if (route === '/@id/__x00__dispatch-custom-platform') return { platform: true };
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
    const owner = /\/features\/custom\/([a-z0-9]+)\//i.exec(file)?.[1];
    if (owner) return { owner: owner.toLowerCase() };
    // Builds copy private source and identifiers outside their owning feature folder.
    // Those artifacts are not development browser inputs, including staged builds.
    if (/(?:^|\/)(?:\.build(?:-[^/]*)?|\.test-build(?:-[^/]*)?|target)(?:\/|$)/.test(file))
      return { platform: true };
    // A later middleware must not turn an undecoded private path into a public one.
    if (route.includes('%')) return { platform: true };
    const relative = path.relative(root, file).replaceAll('\\', '/');
    const aliases = [relative, relative.replace(/\.(?:mjs|cjs|js)$/i, '.ts'), relative + '.ts'];
    if (aliases.some((file) => inventories.has(file.toLowerCase()))) return { platform: true };
    // Browser modules are the public frontends and API TypeScript. Other repository
    // files can contain copied private identifiers (schemas, fixtures and tooling).
    const browser =
      /^(?:app\/frontend|(?:(?:core|features|collectors)\/[^/]+|mcp)\/(?:integrations\/[^/]+\/)?frontend)(?:\/|$)/.test(
        relative,
      ) ||
      /^(?:(?:core|features|collectors)\/[^/]+|mcp)\/(?:integrations\/[^/]+\/)?api\/.*\.[cm]?[jt]sx?$/.test(
        relative,
      ) ||
      /(?:^|\/)node_modules(?:\/|$)/.test(file);
    return browser ? undefined : { platform: true };
  };
}

type SourceGuard = ReturnType<typeof privateSources>;
type HotGuard = { sources: Map<string, SourceGuard>; privateSeen: boolean };
const hotGuards = new WeakMap<ViteDevServer['ws'], HotGuard>();

/** Private edits reload manually. Keep shared HMR, with private paths and error frames
 * out of its broadcasts and replay buffer; detailed errors remain in terminal logs. */
function protectHotChannel(server: ViteDevServer, root: string, source: SourceGuard) {
  root = fs.realpathSync(root);
  const existing = hotGuards.get(server.ws);
  if (existing) {
    existing.sources.set(root, source);
    existing.privateSeen ||= fs.existsSync(path.join(root, 'features/custom'));
    return;
  }
  const guard: HotGuard = {
    sources: new Map([[root, source]]),
    privateSeen: fs.existsSync(path.join(root, 'features/custom')),
  };
  hotGuards.set(server.ws, guard);
  const privatePath = (file?: string) =>
    Boolean(
      file &&
      file !== '*' &&
      [...guard.sources].some(
        ([directory, read]) =>
          read(file) ||
          ((file === directory || file.startsWith(directory + path.sep)) && read('/@fs' + file)),
      ),
    );
  const send = server.ws.send.bind(server.ws);
  server.ws.send = ((payload: HotPayload | string, data?: unknown) => {
    if (typeof payload === 'string') return send(payload, data);
    guard.privateSeen ||= [...guard.sources.keys()].some((directory) =>
      fs.existsSync(path.join(directory, 'features/custom')),
    );
    if (payload.type === 'error' && guard.privateSeen) return;
    if (payload.type === 'update') {
      const updates = payload.updates.filter(
        (update) =>
          ![
            update.path,
            update.acceptedPath,
            update.firstInvalidatedBy,
            ...(update.invalidates ?? []),
          ].some(privatePath),
      );
      if (!updates.length) return;
      payload = { ...payload, updates };
    } else if (payload.type === 'prune') {
      const paths = payload.paths.filter((file) => !privatePath(file));
      if (!paths.length) return;
      payload = { ...payload, paths };
    } else if (
      payload.type === 'full-reload' &&
      (privatePath(payload.path) || privatePath(payload.triggeredBy))
    )
      return;
    else if (payload.type === 'bundled-dev-update' && guard.privateSeen) return;
    send(payload);
  }) as typeof server.ws.send;
}

/** npm run dev protects its outer server and preview cookies. Standalone Vite has no
 * outer server, so it must protect the same source requests with its normal session. */
export function privateSourceGuard(root: string, backend: string): Plugin {
  const audience = privateSources(root);
  let channel: ViteDevServer['ws'] | undefined;
  return {
    name: 'dispatch-private-sources',
    enforce: 'pre',
    hotUpdate({ file }) {
      if (audience('/@fs' + file)) {
        const guard = channel && hotGuards.get(channel);
        if (guard && /\/features\/custom\//i.test(file)) guard.privateSeen = true;
        return [];
      }
    },
    configureServer(server) {
      protectHotChannel(server, root, audience);
      channel = server.ws;
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
    // Node's fetch replaces Host. Authenticate at the browser's DSP authority rather
    // than the loopback backend authority.
    const session = await new Promise<
      | {
          user: { platformOwner: boolean };
          security: { required: boolean; verified: boolean };
          dsps: { code: string | null; status: string }[];
        }
      | undefined
    >((resolve) => {
      const outgoing = http.get(
        `${backend}/api/session`,
        {
          headers: { host: request.headers.host!, cookie },
          signal: AbortSignal.timeout(10_000),
        },
        (response) => {
          response.on('error', () => resolve(undefined));
          if (response.statusCode !== 200) {
            response.resume();
            resolve(undefined);
            return;
          }
          let body = '';
          response.setEncoding('utf8');
          response.on('data', (chunk) => {
            body += chunk;
          });
          response.on('end', () => {
            try {
              resolve(JSON.parse(body));
            } catch {
              resolve(undefined);
            }
          });
        },
      );
      outgoing.on('error', () => resolve(undefined));
    });
    if (!session) return false;
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
