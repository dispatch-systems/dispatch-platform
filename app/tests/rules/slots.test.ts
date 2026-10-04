import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { pathToFileURL } from 'node:url';
import {
  calls,
  collections,
  collectorManifests,
  featureManifests,
  literals,
  productRust,
  stringOf,
  type Span,
} from './support/manifests.js';
import { holds } from './support/holds.js';
import { files, ownerOf, root } from './support/repo.js';
import { closing, lexFile, literalAt } from './support/rust.js';

// Every contribution to a slot carries an id that is its own: two owners claiming one would
// collide in the catalog, the role sheet, the router, the agent API or the job queue.
type Contribution = { id: string; at: string };
function collisions(kind: string, contributions: Contribution[]): string[] {
  const by = new Map<string, Set<string>>();
  for (const { id, at } of contributions) by.set(id, new Set([...(by.get(id) ?? []), at]));
  return [...by]
    .filter(([, at]) => at.size > 1)
    .map(([id, at]) => `${kind} ${id} is contributed by ${[...at].sort().join(', ')}`);
}
const ownerName = (file: string) => ownerOf(file)?.dir ?? file;
/** Each place a contribution is made, so that one declared once counts once. */
const place = (span: Span | { file: string; offset: number }) =>
  `${span.file}:${'offset' in span ? span.offset : span.start}`;

test('every switch, tab and connection on the DSPs page has an id of its own', () => {
  const ids: Contribution[] = [
    ...featureManifests().flatMap((manifest) => [
      ...(manifest.switch ? [{ id: manifest.switch, at: `${manifest.owner.dir} switch` }] : []),
      ...manifest.tabs.map((id) => ({ id, at: `${manifest.owner.dir} tab` })),
    ]),
    ...collectorManifests().flatMap((manifest) =>
      manifest.id ? [{ id: manifest.id, at: `${manifest.owner.dir} connection` }] : [],
    ),
  ];
  assert(ids.length > 10, `found only ${ids.length} switches`);
  holds('slots', 'switches', collisions('switch', ids));
});

test('every permission has an id of its own', () => {
  const ids: Contribution[] = [
    ...calls('perm').flatMap((span) => {
      const literal = literalAt(lexFile(span.file), span.start);
      return literal ? [{ id: literal.value, at: place(span) }] : [];
    }),
    ...literals('Permission').flatMap(({ file, offset, fields }) => {
      const id = fields.get('id');
      const value = id && stringOf(id);
      return value ? [{ id: value, at: place({ file, offset }) }] : [];
    }),
  ];
  assert(ids.length > 15, `found only ${ids.length} permissions`);
  holds('slots', 'permissions', collisions('permission', ids));
});

// The HTTP routes: what each registration helper registers, by method and path.
const registrations: Record<string, { method?: string; path: number }> = {
  read: { method: 'GET', path: 0 },
  write: { method: 'POST', path: 0 },
  async_get: { method: 'GET', path: 0 },
  async_post: { method: 'POST', path: 0 },
  probe: { method: 'GET', path: 0 },
  protocol: { path: 1 },
  agent_protocol: { path: 1 },
};
/** A call's arguments, each as a span. */
function argumentsOf(span: Span): Span[] {
  const { masked } = lexFile(span.file);
  const out: Span[] = [];
  let start = span.start;
  const stack: string[] = [];
  for (let at = span.start; at <= span.end; at++) {
    const char = masked[at]!;
    if ('([{'.includes(char)) stack.push(char);
    else if (')]}'.includes(char) && stack.length) stack.pop();
    else if ((char === ',' && !stack.length) || at === span.end) {
      if (masked.slice(start, at).trim()) out.push({ file: span.file, start, end: at });
      start = at + 1;
    }
  }
  return out;
}
const methodOf = (span: Span) =>
  /Method\s*::\s*([A-Z]+)/.exec(lexFile(span.file).masked.slice(span.start, span.end))?.[1];
/** A path argument: a literal, `path(prefix, "/suffix")`, or a helper's parameter. */
function pathOf(span: Span, parameter?: string): { literal?: string; suffix: string } | undefined {
  const text = lexFile(span.file).masked.slice(span.start, span.end).trim();
  const literal = literalAt(lexFile(span.file), span.start);
  if (literal && /^"/.test(text)) return { literal: literal.value, suffix: '' };
  if (parameter && text === parameter) return { suffix: '' };
  const joined = /^path\s*\(\s*([A-Za-z_]+)\s*,/.exec(text);
  if (joined && joined[1] === parameter) {
    const at = span.start + lexFile(span.file).masked.slice(span.start).indexOf(',') + 1;
    const suffix = literalAt(lexFile(span.file), at);
    return suffix ? { suffix: suffix.value } : undefined;
  }
  return undefined;
}
type Template = { method: string; suffix: string };
/** Helpers such as `schedule_routes(prefix, …)`: the routes each registers under its path. */
function helpers(): Map<string, Template[]> {
  const found = new Map<string, Template[]>();
  const definitions = files.filter(productRust).flatMap((file) => {
    const { masked } = lexFile(file);
    return [
      ...masked.matchAll(
        /\bfn\s+([a-z_][a-z0-9_]*)\s*\(\s*([a-z_][a-z0-9_]*)\s*:\s*&\s*'static\s+str[^)]*\)\s*->\s*(?:Vec\s*<\s*)?Route\b[^{]*\{/g,
      ),
    ].map((match) => {
      const open = match.index + match[0].length - 1;
      return {
        name: match[1]!,
        parameter: match[2]!,
        body: { file, start: open, end: closing(masked, open) },
      };
    });
  });
  // Helpers may build on each other; a few passes reach every one.
  for (let pass = 0; pass < 4; pass++)
    for (const { name, parameter, body } of definitions) {
      const templates: Template[] = [];
      for (const [callee, registration] of [
        ...Object.entries(registrations),
        ...[...found.keys()].map((helper) => [helper, { path: 0 }] as const),
      ]) {
        const { masked } = lexFile(body.file);
        for (const match of masked
          .slice(body.start, body.end)
          .matchAll(new RegExp(`(?<![A-Za-z0-9_:.])${callee}\\s*\\(`, 'g'))) {
          const open = body.start + match.index + match[0].length - 1;
          const args = argumentsOf({
            file: body.file,
            start: open + 1,
            end: closing(masked, open),
          });
          const at = args[registration.path];
          const path = at && pathOf(at, parameter);
          if (!path || path.literal !== undefined) continue;
          const helper = found.get(callee);
          if (helper)
            templates.push(
              ...helper.map((t) => ({ method: t.method, suffix: path.suffix + t.suffix })),
            );
          else {
            const method = 'method' in registration ? registration.method : methodOf(args[0]!);
            if (method) templates.push({ method, suffix: path.suffix });
          }
        }
      }
      if (templates.length) found.set(name, templates);
    }
  return found;
}

test('every HTTP route has a method and path of its own', () => {
  const templates = helpers();
  const routes: Contribution[] = [];
  for (const [callee, registration] of [
    ...Object.entries(registrations),
    ...[...templates.keys()].map((helper) => [helper, { path: 0 }] as const),
  ])
    for (const span of calls(callee)) {
      const args = argumentsOf(span);
      const at = args[registration.path];
      const path = at && pathOf(at);
      if (!path?.literal) continue;
      const methods = templates.get(callee) ?? [
        {
          method: ('method' in registration && registration.method) || methodOf(args[0]!) || '?',
          suffix: '',
        },
      ];
      for (const { method, suffix } of methods)
        routes.push({ id: `${method} ${path.literal}${suffix}`, at: place(span) });
    }
  assert(routes.length > 100, `found only ${routes.length} routes`);
  holds('slots', 'routes', collisions('route', routes));
});

// The frontend's manifests, as they load: each page's address and each Settings tab. Every
// owner's frontend/feature.ts counts, listed in the app or not.
type FrontendManifest = {
  routes?: readonly { id: string; scope: string }[];
  settingsTabs?: readonly { id: string }[];
};
async function frontendContributions() {
  const addresses: Contribution[] = [];
  const settingsTabs: Contribution[] = [];
  const manifests = files.filter((file) =>
    /^(core|features|collectors)\/[^/]+\/frontend\/feature\.ts$/.test(file),
  );
  for (const file of manifests) {
    const { feature } = (await import(pathToFileURL(path.join(root, file)).href)) as {
      feature: FrontendManifest;
    };
    for (const route of feature.routes ?? [])
      addresses.push({ id: `${route.scope}/${route.id}`, at: ownerName(file) });
    for (const tab of feature.settingsTabs ?? [])
      settingsTabs.push({ id: tab.id, at: ownerName(file) });
  }
  return { addresses, settingsTabs };
}

test("every page's address and every Settings tab has an id of its own", async () => {
  const { addresses, settingsTabs } = await frontendContributions();
  assert(
    addresses.length > 10 && settingsTabs.length > 3,
    'the frontend manifests declare pages and tabs',
  );
  // One owner may not declare an id twice either.
  const twice = (kind: string, list: Contribution[]) =>
    collisions(
      kind,
      list.map((item, index) => ({ id: item.id, at: `${item.at}#${index}` })),
    ).map((violation) => violation.replace(/#\d+/g, ''));
  holds('slots', 'addresses', [
    ...twice('address', addresses),
    ...twice('settings tab', settingsTabs),
  ]);
});

test('every MCP endpoint, tool and read toggle has an id of its own', () => {
  const endpoints = literals('Endpoint').filter(({ fields }) => fields.has('tool'));
  const field = (name: string) =>
    endpoints.flatMap(({ file, offset, fields }) => {
      const value = fields.get(name) && stringOf(fields.get(name)!);
      return value ? [{ id: value, at: place({ file, offset }) }] : [];
    });
  const toggles = literals('ReadToggle').flatMap(({ file, offset, fields }) => {
    const value = fields.get('id') && stringOf(fields.get('id')!);
    return value ? [{ id: value, at: place({ file, offset }) }] : [];
  });
  assert(endpoints.length > 10, `found only ${endpoints.length} MCP endpoints`);
  holds('slots', 'mcp', [
    ...collisions('MCP endpoint', field('id')),
    ...collisions('MCP tool', field('tool')),
    ...collisions('MCP path', field('path')),
    ...collisions('read toggle', toggles),
  ]);
});

test('every collection has a job kind of its own', () => {
  const kinds = collections().map(({ jobKind, file, offset }) => ({
    id: jobKind,
    at: place({ file, offset }),
  }));
  assert(kinds.length > 0, 'the collectors declare collections');
  holds('slots', 'job kinds', collisions('job kind', kinds));
});

// An operator command goes to the family whose prefix it begins with, so a family's prefix
// must not begin another owner's command or family.
test('every operator command belongs to one owner', () => {
  const prefixes = literals('Commands').flatMap(({ file, fields }) => {
    const prefix = fields.get('prefix') && stringOf(fields.get('prefix')!);
    return prefix ? [{ prefix, owner: ownerName(file) }] : [];
  });
  const names = files
    .filter((file) => /\/cli\.rs$/.test(file) && productRust(file))
    .flatMap((file) => {
      // Its names: what `command ==` compares, and the arms that match a command.
      const { masked, literals: strings } = lexFile(file);
      return strings
        .filter(
          (literal) =>
            /^[a-z][a-z0-9-]*$/.test(literal.value) &&
            (/\bcommand\s*==\s*$/.test(
              masked.slice(Math.max(0, literal.start - 20), literal.start),
            ) ||
              /^\s*(\||=>)/.test(masked.slice(literal.end, literal.end + 20))),
        )
        .map((literal) => ({ name: literal.value, owner: ownerName(file) }));
    });
  const wrong = [
    ...collisions(
      'command',
      names.map(({ name, owner }) => ({ id: name, at: owner })),
    ),
    ...prefixes.flatMap(({ prefix, owner }) => [
      ...names
        .filter((name) => name.owner !== owner && name.name.startsWith(prefix))
        .map(
          (name) => `command ${name.name} of ${name.owner} begins with ${owner}'s prefix ${prefix}`,
        ),
      ...prefixes
        .filter((other) => other.owner !== owner && other.prefix.startsWith(prefix))
        .map((other) => `${other.owner}'s prefix ${other.prefix} begins with ${owner}'s ${prefix}`),
    ]),
  ];
  assert(names.length > 3, `found only ${names.length} commands`);
  holds('slots', 'commands', wrong);
});
