import fs from 'node:fs';
import path from 'node:path';
import { dashboardGraphSizes, type DashboardAssets } from './dashboard-assets.js';

type Graph = {
  name: string;
  /** The entries of Vite's manifest the page loads beside `index.html`, as it names them. */
  entries: string[];
  /** The bytes its JavaScript stays under, before compression and as Brotli sends it. */
  raw: number;
  transferred: number;
  /** The bytes its CSS stays under, before compression. */
  styles: number;
};
/** The dashboard's page-size budgets: what each page loads before it shows, and the bytes that
 * stays under. The artifact test holds the release build to them, and `dispatchdev check` a
 * change's dashboard before it is pushed. */
export const graphs: Graph[] = [
  { name: 'initial', entries: [], raw: 500_000, transferred: 115_000, styles: 30_000 },
  {
    name: 'DSP picker',
    entries: ['../../core/platform_owner/frontend/dsps/picker.ts'],
    raw: 500_000,
    transferred: 120_000,
    styles: 60_000,
  },
  {
    name: 'Settings profile',
    // The page, the Profile panel and Driver Match's tab badge load together.
    entries: [
      '../../features/settings/frontend/index.ts',
      '../../core/accounts/frontend/settings/ProfileBadge.tsx',
      '../../features/driver_match/frontend/badge.ts',
    ],
    raw: 445_000,
    transferred: 132_000,
    styles: 60_000,
  },
  {
    name: 'daily Timecard',
    entries: [
      '../../features/timecard/frontend/index.ts',
      '../../features/timecard/frontend/tabs/daily/TimecardsPage.tsx',
    ],
    raw: 500_000,
    transferred: 150_000,
    styles: 60_000,
  },
  {
    name: 'Collectors',
    entries: ['../../core/collection/frontend/collectors/index.ts'],
    raw: 490_000,
    transferred: 145_000,
    styles: 60_000,
  },
];
/** The bytes the first page's common stylesheet stays under; feature CSS loads with its route. */
export const initialStylesheet = 30_000;

type Asset = {
  name: string;
  /** The built file in `assets/`, by its name; none for the script `index.html` loads. */
  file?: RegExp;
  /** Whether the budget holds it as Brotli sends it, or before compression. */
  compressed: boolean;
  budget: number;
  /** What the artifact test says when it isn't. */
  message: string;
};
/** Single files held to a budget of their own. */
export const assets: Asset[] = [
  {
    name: 'main script',
    compressed: false,
    budget: 400_000,
    message: 'main dashboard JavaScript stays below 400 KB before compression',
  },
  {
    name: 'map',
    file: /^route-map-.*\.svg$/,
    compressed: true,
    budget: 900_000,
    message: 'compressed vector map stays within its initial-load budget',
  },
  {
    name: 'van',
    file: /^login-van-.*\.glb$/,
    compressed: true,
    budget: 300_000,
    message: 'compressed van transfer stays under 300 KB',
  },
  {
    name: 'renderer',
    file: /^renderer-.*\.js$/,
    compressed: true,
    budget: 200_000,
    message: 'deferred renderer transfer stays under 200 KB',
  },
];
/** The budget of the asset named `name`. */
export const assetBudget = (name: string) => assets.find((each) => each.name === name)!;

/** The bytes the file at `file` takes as it is sent: Brotli's when it was worth writing, as the
 * server sends, else the file's own. */
function sent(file: string, compressed: boolean) {
  const brotli = `${file}.br`;
  return fs.statSync(compressed && fs.existsSync(brotli) ? brotli : file).size;
}
/** Each asset's size in the dashboard built at `root`, or none where the build has no such file. */
export const assetSizes = (root: string) => {
  const folder = path.join(root, 'assets');
  const names = fs.readdirSync(folder);
  const main = fs
    .readFileSync(path.join(root, 'index.html'), 'utf8')
    .match(/src="\.?\/assets\/([^"]+\.js)"/)?.[1];
  return assets.map((each) => {
    const name = each.file ? names.find((file) => each.file!.test(file)) : main;
    return { asset: each, size: name ? sent(path.join(folder, name), each.compressed) : undefined };
  });
};

/** Each graph's sizes in the dashboard built at `root`, whose chunks `assets` lists. */
export const graphSizes = (root: string, assets: DashboardAssets) =>
  graphs.map((graph) => ({
    graph,
    size: dashboardGraphSizes(root, assets, ['index.html', ...graph.entries]),
  }));
export type GraphSizes = ReturnType<typeof graphSizes>;
export type AssetSizes = ReturnType<typeof assetSizes>;
/** Everything the budgets hold a build to. */
export type Sizes = { graphs: GraphSizes; assets: AssetSizes };

type Limit = { what: string; label: string; size: number; budget: number; compressed?: boolean };
/** A graph's or an asset's budgets, each with its size: a size passes while it stays under. */
type Row = { name: string; limits: Limit[] };
const graphLimits = ({ graph, size }: GraphSizes[number]): Limit[] => [
  { what: 'JavaScript', label: 'JavaScript', size: size.scripts.raw, budget: graph.raw },
  {
    what: 'transferred JavaScript',
    label: 'transferred',
    size: size.scripts.transferred,
    budget: graph.transferred,
    compressed: true,
  },
  { what: 'CSS', label: 'CSS', size: size.styles.raw, budget: graph.styles },
];
/** Every budget a build is held to, by what it holds: an asset the build lacks has none. */
const rows = (sizes: Sizes): Row[] => [
  ...sizes.graphs.map((graph) => ({ name: graph.graph.name, limits: graphLimits(graph) })),
  ...sizes.assets.map(({ asset, size }) => ({
    name: asset.name,
    limits:
      size === undefined
        ? []
        : [
            {
              what: asset.compressed ? 'transferred size' : 'size',
              label: asset.compressed ? 'transferred' : 'raw',
              size,
              budget: asset.budget,
              compressed: asset.compressed,
            },
          ],
  })),
];
/** Whether any budget is full or over. */
export const overBudget = (sizes: Sizes) =>
  rows(sizes).some((row) => row.limits.some(({ size, budget }) => size >= budget));
/** Under 1% of its budget left is a warning: the next change can take it. */
const thin = (left: number, budget: number) => left < budget / 100;
const bytes = (n: number) => n.toLocaleString('en-US');

/** Where `main` stood against a budget a change is over: its size, or why it wasn't measured. */
function mainAgainst(
  main: Sizes | string,
  name: string,
  what: string,
  { size: now, budget }: { size: number; budget: number },
) {
  if (typeof main === 'string') return `  main: ${main}.`;
  const size = rows(main)
    .find((row) => row.name === name)
    ?.limits.find((limit) => limit.what === what)?.size;
  if (size === undefined) return `  main: no ${what} of ${name}.`;
  const left = budget - size;
  const stood =
    left <= 0
      ? `already ${bytes(-left)} over: main's to fix, not this change's`
      : thin(left, budget)
        ? `${bytes(left)} left: main is at the edge already`
        : `${bytes(left)} left: this change adds ${bytes(now - size)}`;
  return `  main: ${bytes(size)} bytes, ${stood}.`;
}

/** What `dispatchdev check` says of a change's build: a line for each graph and asset, with each
 * size and what its budget leaves; a warning for each budget under 1% from full; and each one
 * over, with by how much and where `main` stood against it, where it was measured. */
export function pageSizeReport(change: Sizes, main?: Sizes | string) {
  const all = rows(change);
  const width = Math.max(...all.map(({ name }) => name.length));
  const lines: string[] = [];
  const warnings: string[] = [];
  let thinTransfer = false;
  for (const { name, limits } of all) {
    const measured = limits.map((limit) => {
      const left = limit.budget - limit.size;
      const room = left > 0 ? `${bytes(left)} left` : `${bytes(-left)} over`;
      return `${limit.label} ${bytes(limit.size)}/${bytes(limit.budget)} (${room})`;
    });
    lines.push(`${`${name}:`.padEnd(width + 1)} ${measured.join(', ') || 'not in this build'}`);
    for (const { what, size, budget, compressed } of limits) {
      const left = budget - size;
      if (!thin(left, budget)) continue;
      if (compressed) thinTransfer = true;
      if (left > 0) {
        warnings.push(
          `Warning: ${name}'s ${what} has ${bytes(left)} bytes left, under 1% of its ${bytes(budget)}.`,
        );
        continue;
      }
      warnings.push(
        `✖ ${name}'s ${what} is ${bytes(size)} bytes, ${bytes(-left)} over its budget of ${bytes(budget)}.`,
      );
      if (main !== undefined) warnings.push(mainAgainst(main, name, what, { size, budget }));
    }
  }
  if (thinTransfer)
    warnings.push(
      'Compressed sizes vary by about 100 bytes from hashed file names alone: under 150 bytes ' +
        'of transferred headroom is no real margin.',
    );
  return [...lines, ...warnings];
}
