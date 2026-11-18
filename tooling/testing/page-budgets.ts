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

/** Each graph's sizes in the dashboard built at `root`, whose chunks `assets` lists. */
export const graphSizes = (root: string, assets: DashboardAssets) =>
  graphs.map((graph) => ({
    graph,
    size: dashboardGraphSizes(root, assets, ['index.html', ...graph.entries]),
  }));
export type GraphSizes = ReturnType<typeof graphSizes>;

/** A graph's budgets, each with its size: a size passes while it stays under its budget. */
const limits = ({ graph, size }: GraphSizes[number]) => [
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
/** Whether any graph is over a budget. */
export const overBudget = (sizes: GraphSizes) =>
  sizes.some((graph) => limits(graph).some(({ size, budget }) => size >= budget));
/** Under 1% of its budget left is a warning: the next change can take it. */
const thin = (left: number, budget: number) => left < budget / 100;
const bytes = (n: number) => n.toLocaleString('en-US');

/** Where `main` stood against a budget a change is over: its size, or why it wasn't measured. */
function mainAgainst(
  main: GraphSizes | string,
  graph: string,
  what: string,
  { size: now, budget }: { size: number; budget: number },
) {
  if (typeof main === 'string') return `  main: ${main}.`;
  const before = main.find((other) => other.graph.name === graph);
  const size = before && limits(before).find((limit) => limit.what === what)!.size;
  if (size === undefined) return '  main: no such graph.';
  const left = budget - size;
  const stood =
    left <= 0
      ? `already ${bytes(-left)} over: main's to fix, not this change's`
      : thin(left, budget)
        ? `${bytes(left)} left: main is at the edge already`
        : `${bytes(left)} left: this change adds ${bytes(now - size)}`;
  return `  main: ${bytes(size)} bytes, ${stood}.`;
}

/** What `dispatchdev check` says of a change's graphs: a line for each, with each size and what
 * its budget leaves; a warning for each budget under 1% from full; and each one over, with by how
 * much and where `main` stood against it, where it was measured. */
export function pageSizeReport(change: GraphSizes, main?: GraphSizes | string) {
  const width = Math.max(...change.map(({ graph }) => graph.name.length));
  const lines: string[] = [];
  const warnings: string[] = [];
  let thinTransfer = false;
  for (const graph of change) {
    const name = graph.graph.name;
    const measured = limits(graph).map((limit) => {
      const left = limit.budget - limit.size;
      const room = left > 0 ? `${bytes(left)} left` : `${bytes(-left)} over`;
      return `${limit.label} ${bytes(limit.size)}/${bytes(limit.budget)} (${room})`;
    });
    lines.push(`${`${name}:`.padEnd(width + 1)} ${measured.join(', ')}`);
    for (const { what, size, budget, compressed } of limits(graph)) {
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
        `\u2716 ${name}'s ${what} is ${bytes(size)} bytes, ${bytes(-left)} over its budget of ${bytes(budget)}.`,
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
