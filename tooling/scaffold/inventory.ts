import fs from 'node:fs';
import path from 'node:path';

// `dispatchdev list <what>` and `dispatchdev explain <owner>`: what the product has, read from
// what `dispatchdev generate` writes (app/generated/features.json, the catalog's snapshots,
// the people data) and the slot catalogue, so the answer is the build's, not a guess from the
// source. Each answers rows of plain values: a line each, or with --json the rows as JSON.

const FEATURES = 'app/generated/features.json';
const SNAPSHOTS = 'app/tests/backend/snapshots';
const PEOPLE = 'core/manifest/api/generated/people-data.ts';
const CATALOGUE = 'core/manifest/README.md';

type Feature = {
  collectors: string[];
  depends_on: string[];
  frontend: boolean;
  keeps: string[];
  migrations: Record<string, { id: number; name: string }[]>;
  permissions: string[];
  slots: Record<string, unknown>;
  switch: string | null;
  mandatory: boolean;
  tables: Record<string, string[]>;
  subfeatures: string[];
};
type Page = {
  id: string;
  label: string;
  page?: string;
  tab?: boolean;
  permissions: string[];
  requires: string[];
};
type Tool = { name: string; title: string; features: string[]; changes: boolean; scope: string };
export type Row = Record<string, string | number | boolean | string[] | null>;

const read = (root: string, file: string) => fs.readFileSync(path.join(root, file), 'utf8');
const json = <T>(root: string, file: string): T => JSON.parse(read(root, file)) as T;
/** Where a contract's crate is, from its type's path: a feature's contract crate, or core. */
const contractDir = (contract: string) => {
  const crate = contract.split('::')[0]!;
  const feature = /^dispatch_(\w+)_contract$/.exec(crate)?.[1];
  return feature ? `features/${feature}/contract` : crate === 'dispatch_core' ? 'core' : crate;
};
const features = (root: string) => json<Record<string, Feature>>(root, FEATURES);
const pages = (root: string) => {
  const catalog = json<{ pages: Page[]; subfeatures: Page[] }>(
    root,
    `${SNAPSHOTS}/catalog-pages.json`,
  );
  return [...catalog.pages, ...catalog.subfeatures];
};
const tools = (root: string) => json<Tool[]>(root, `${SNAPSHOTS}/agent-tools.json`);
/** The rows of the catalogue's table under `heading`: each slot with its module and what it adds. */
function slotTable(root: string, heading: string) {
  const text = read(root, CATALOGUE).split(`## ${heading}\n`)[1] ?? '';
  return text
    .split('\n## ')[0]!
    .split('\n')
    .filter((line) => /^\| `/.test(line))
    .map((line) => line.split('|').map((cell) => cell.trim().replace(/`/g, '')))
    .map(([, slot, module, adds]) => ({ slot: slot!, module: module!, adds: adds! }));
}
/** The `{ id, label }` rows of one of the people data's lists. */
function peopleList(root: string, list: string) {
  const text =
    read(root, PEOPLE).split(`export const ${list} = [`)[1]?.split('] as const')[0] ?? '';
  return [
    ...text.matchAll(/\{ id: "([^"]+)", label: "([^"]+)"(?:, unit: (?:"([^"]+)"|null))?/g),
  ].map(([, id, label, unit]) => ({ id: id!, label: label!, unit: unit ?? null }));
}
/** The slots an owner's frontend manifest fills: its `feature` object's own keys. */
export function frontendSlots(source: string) {
  const body = source.split(/export const feature: FrontendFeature = \{\n/)[1]?.split(/\n\};/)[0];
  return [...(body ?? '').matchAll(/^ {2}(?:get )?([a-zA-Z]+)\s*[:,(]/gm)]
    .map(([, key]) => key!)
    .filter((key) => key !== 'name');
}
/** Whether a backend slot holds something: a non-empty list, object or value. */
const filled = (value: unknown): boolean =>
  Array.isArray(value)
    ? value.length > 0
    : value && typeof value === 'object'
      ? Object.values(value).some(filled)
      : Boolean(value);

/** What `dispatchdev list` lists, each with what it reads. */
export const LISTS: Record<string, { about: string; rows: (root: string) => Row[] }> = {
  features: {
    about: 'every feature, whether every DSP has it, what it uses and keeps',
    rows: (root) =>
      Object.entries(features(root)).map(([name, feature]) => ({
        name,
        mandatory: feature.mandatory,
        uses: feature.depends_on,
        keeps: feature.keeps,
        permissions: feature.permissions.length,
      })),
  },
  collectors: {
    about: 'every collector, and the features keeping its collections',
    rows: (root) =>
      fs
        .readdirSync(path.join(root, 'collectors'))
        .filter((name) => fs.existsSync(path.join(root, 'collectors', name, 'Cargo.toml')))
        .sort()
        .map((name) => ({
          name,
          keptBy: Object.entries(features(root))
            .filter(([, feature]) => feature.collectors.includes(name))
            .map(([feature]) => feature),
        })),
  },
  slots: {
    about: "every slot an owner's manifest fills, backend and frontend, and what it adds",
    rows: (root) => [
      ...slotTable(root, 'Backend slots').map((row) => ({ side: 'backend', ...row })),
      ...slotTable(root, 'Frontend slots').map((row) => ({ side: 'frontend', ...row })),
    ],
  },
  exposes: {
    about: 'what each feature exposes for others to use, and where its contract is',
    rows: (root) =>
      Object.entries(features(root)).flatMap(([name, feature]) =>
        ((feature.slots.exposes ?? []) as { name: string; contract: string; about: string }[]).map(
          (exposed) => ({
            name: exposed.name,
            feature: name,
            contract: exposed.contract,
            in: contractDir(exposed.contract),
            about: exposed.about,
          }),
        ),
      ),
  },
  integrations: {
    about: "every integration: what a feature does through a connection's account",
    rows: (root) =>
      Object.entries(features(root)).flatMap(([name, feature]) =>
        (feature.slots.integrations as [string, string][]).map(([id, connection]) => ({
          id,
          feature: name,
          connection,
        })),
      ),
  },
  tools: {
    about: 'every tool agents use, what it is about and the features it needs',
    rows: (root) =>
      tools(root).map((tool) => ({
        name: tool.name,
        scope: tool.scope,
        features: tool.features,
        changes: tool.changes,
      })),
  },
  sources: {
    about: "the people's sources and kinds of data Driver Match tells people apart by",
    rows: (root) => [
      ...peopleList(root, 'peopleSources').map(({ id, label }) => ({ kind: 'source', id, label })),
      ...peopleList(root, 'peopleKinds').map(({ id, label, unit }) => ({
        kind: 'data',
        id,
        label,
        unit,
        feature:
          Object.entries(features(root)).find(([, feature]) =>
            (feature.slots.people as string[]).includes(id),
          )?.[0] ?? null,
      })),
    ],
  },
  permissions: {
    about: 'every permission on the role sheet',
    rows: (root) =>
      json<{ permissions: [string, string][] }>(
        root,
        `${SNAPSHOTS}/catalog-permissions.json`,
      ).permissions.map(([id, label]) => ({ id, label })),
  },
  pages: {
    about: 'every page and tab, with the permissions that show it',
    rows: (root) =>
      pages(root).map((page) => ({
        id: page.id,
        label: page.label,
        tabOf: page.page ?? null,
        permissions: page.permissions,
      })),
  },
};

/** What a feature or collector is made of, from the build's own lists. */
export function explain(root: string, name: string): Row {
  const all = features(root);
  const feature = all[name];
  const folder = ['features', 'collectors']
    .map((owner) => `${owner}/${name}`)
    .find((dir) => fs.existsSync(path.join(root, dir)));
  if (!folder)
    throw new Error(`No feature or collector ${name}: dispatchdev list features names them`);
  const manifest = `${folder}/frontend/feature.ts`;
  const frontend = fs.existsSync(path.join(root, manifest))
    ? frontendSlots(read(root, manifest))
    : [];
  const usedBy = Object.entries(all)
    .filter(([, other]) => other.depends_on.includes(name) || other.collectors.includes(name))
    .map(([other]) => other);
  const toolsUsing = tools(root)
    .filter((tool) => tool.features.some((id) => id.split('.')[0] === name))
    .map((tool) => tool.name);
  if (!feature)
    return { name, folder, owner: 'collector', keptBy: usedBy, frontend, tools: toolsUsing };
  const backend = Object.entries(feature.slots)
    .filter(([slot, value]) => slot !== 'routes' && filled(value))
    .map(([slot]) => slot);
  const routes = feature.slots.routes as string[];
  return {
    name,
    folder,
    owner: 'feature',
    mandatory: feature.mandatory,
    uses: feature.depends_on,
    usedBy,
    keeps: feature.keeps,
    permissions: feature.permissions,
    pages: pages(root)
      .filter((page) => (page.page ?? page.id).split('.')[0] === name || page.id === name)
      .map((page) => (page.page ? `${page.id} (tab)` : page.id)),
    subfeatures: feature.subfeatures,
    tables: Object.entries(feature.tables).map(([db, tables]) => `${db}: ${tables.join(', ')}`),
    routes,
    exposes: ((feature.slots.exposes ?? []) as { name: string }[]).map((exposed) => exposed.name),
    integrations: (feature.slots.integrations as [string, string][]).map(
      ([id, connection]) => `${id} (through ${connection})`,
    ),
    people: feature.slots.people as string[],
    backend,
    frontend,
    tools: toolsUsing,
  };
}

/** Rows as lines: each row's values in columns, lists joined. */
export function table(rows: Row[]) {
  if (!rows.length) return 'None.';
  const keys = Object.keys(rows[0]!);
  const cell = (value: Row[string]) =>
    Array.isArray(value) ? value.join(', ') || '-' : value === null ? '-' : String(value);
  const widths = keys.map((key) =>
    Math.max(key.length, ...rows.map((row) => cell(row[key] ?? null).length)),
  );
  const line = (cells: string[]) =>
    cells
      .map((text, index) => (index === cells.length - 1 ? text : text.padEnd(widths[index]!)))
      .join('  ')
      .trimEnd();
  return [line(keys), ...rows.map((row) => line(keys.map((key) => cell(row[key] ?? null))))].join(
    '\n',
  );
}
/** One owner's parts as lines: a field a line, routes and long lists one to a line under it. */
export function fields(row: Row) {
  const lines: string[] = [];
  for (const [key, value] of Object.entries(row)) {
    if (Array.isArray(value) && (value.length > 4 || value.some((item) => item.includes(', '))))
      lines.push(`${key}:`, ...value.map((item) => `  ${item}`));
    else
      lines.push(
        `${key}: ${Array.isArray(value) ? value.join(', ') || '-' : value === null ? '-' : String(value)}`,
      );
  }
  return lines.join('\n');
}
