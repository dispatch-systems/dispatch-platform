import fs from 'node:fs';
import path from 'node:path';
import { UsageError } from './common.js';

export const read = (root: string, file: string) => fs.readFileSync(path.join(root, file), 'utf8');
export const exists = (root: string, file: string) => fs.existsSync(path.join(root, file));
/** The directories in `dir`, sorted; none when it does not exist. */
function directories(root: string, dir: string) {
  if (!exists(root, dir)) return [];
  return fs
    .readdirSync(path.join(root, dir), { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();
}
/** Every file under `dir` whose path matches, relative to the root. */
export function files(root: string, dir: string, pattern: RegExp): string[] {
  if (!exists(root, dir)) return [];
  return fs
    .readdirSync(path.join(root, dir), { recursive: true, encoding: 'utf8' })
    .map((name) => path.posix.join(dir, name.split(path.sep).join('/')))
    .filter((file) => pattern.test(file) && !/(^|\/)(node_modules|target)\//.test(file))
    .sort();
}
const ownerRoots = ['core', 'collectors', 'features', 'mcp', 'app'];
const rustSources = (root: string) =>
  ownerRoots.flatMap((dir) => files(root, dir, /\.rs$/)).filter((file) => !/\/tests\//.test(file));

export const features = (root: string) =>
  directories(root, 'features').filter((name) => exists(root, `features/${name}/feature.rs`));
export const collectors = (root: string) =>
  directories(root, 'collectors').filter((name) => exists(root, `collectors/${name}/collector.rs`));
export const coreParts = (root: string) => directories(root, 'core');

/** The next free tens after every feature's place. */
export function nextPlace(root: string) {
  let highest = 0;
  for (const name of features(root))
    for (const match of read(root, `features/${name}/feature.rs`).matchAll(/^\s*place:\s*(\d+),/gm))
      highest = Math.max(highest, Number(match[1]));
  return (Math.floor(highest / 10) + 1) * 10;
}
/** The next free tens in the one order every list of permissions follows. */
export function nextPermissionOrder(root: string) {
  let highest = 0;
  for (const file of rustSources(root))
    for (const match of read(root, file).matchAll(
      /\bperm\(\s*"[^"]*",\s*"[^"]*",\s*(\d+)\s*,?\s*\)/g,
    ))
      highest = Math.max(highest, Number(match[1]));
  return (Math.floor(highest / 10) + 1) * 10;
}
/**
 * The order after a feature's own permissions', for those added to them: its next three are
 * free, so a view, collect and manage permission each keep one no other feature holds.
 */
export function ownPermissionOrder(root: string, feature: string) {
  const orders = (text: string) =>
    [...text.matchAll(/\bperm\(\s*"[^"]*",\s*"[^"]*",\s*(\d+)\s*,?\s*\)/g)].map((match) =>
      Number(match[1]),
    );
  const own = orders(read(root, `features/${feature}/feature.rs`));
  if (!own.length) return nextPermissionOrder(root);
  const used = new Set(rustSources(root).flatMap((file) => orders(read(root, file))));
  let order = Math.max(...own) + 1;
  while ([0, 1, 2].some((step) => used.has(order + step))) order++;
  return order;
}
/** The next free tens in the one order the sources of people's IDs follow. */
export function nextSourceOrder(root: string) {
  let highest = 0;
  for (const file of rustSources(root))
    for (const match of read(root, file).matchAll(/\bSourceInfo \{[^}]*?\border:\s*(\d+),/g))
      highest = Math.max(highest, Number(match[1]));
  return (Math.floor(highest / 10) + 1) * 10;
}
/** The next free tens in the one order the kinds of data that name people follow. */
export function nextPeopleOrder(root: string) {
  let highest = 0;
  for (const file of rustSources(root))
    for (const match of read(root, file).matchAll(/\bKindInfo \{[^}]*?\border:\s*(\d+),/g))
      highest = Math.max(highest, Number(match[1]));
  return (Math.floor(highest / 10) + 1) * 10;
}
/**
 * The sources of the IDs people are known by, each with the collector that declares it, as
 * its `SOURCE`: Amazon's transporter IDs are Cortex's.
 */
export function peopleSources(root: string) {
  return collectors(root).flatMap((site) => {
    const source = read(root, `collectors/${site}/collector.rs`);
    const id = /\bSOURCE: Source = Source::new\(&SourceInfo \{\s*id: "([^"]+)"/.exec(source)?.[1];
    return id ? [{ id, site }] : [];
  });
}
/** The collections a collector reads: one directory each under `collections/`. */
export const collectionsOf = (root: string, site: string) =>
  directories(root, `collectors/${site}/collections`);
/**
 * A collector's collection as its `COLLECTIONS` lists it: the schedule Settings' Collectors page
 * sets it by, its label, and the count of a run's metrics its runs are compared per.
 */
export function collectionOf(root: string, site: string, collection: string) {
  const source = read(root, `collectors/${site}/collector.rs`);
  const found = new RegExp(
    `job_kind:\\s*(?:\\w+::)*${collection}::JOB_KIND,\\s*schedule:\\s*"([^"]+)",\\s*` +
      `label:\\s*"([^"]+)",[^}]*?\\bcounted:\\s*Counted::(\\w+)`,
  ).exec(source);
  if (!found) throw new UsageError(`${site}'s COLLECTIONS lists no ${collection}`);
  const [, schedule = '', label = '', counted = ''] = found;
  return { schedule, label, counted: counted.toLowerCase() };
}
/** What a collector's connection supplies to the pages that require it. */
export function capabilitiesOf(root: string, site: string) {
  const source = read(root, `collectors/${site}/collector.rs`);
  const list = /fn capabilities\(&self\)[^{]*\{\s*&\[([^\]]*)\]/.exec(source)?.[1] ?? '';
  return [...list.matchAll(/\bid:\s*"([^"]+)"/g)].map((match) => match[1]!);
}
/**
 * The feature file that keeps `site`'s `collection` already, if any: each collection has one
 * keeper. Another collector may have a collection of the same name.
 */
export function keeperOf(root: string, site: string, collection: string) {
  const names = new RegExp(`\\b${collection}::(\\{[^}]*\\bJOB_KIND\\b|JOB_KIND\\b)`);
  const crate = new RegExp(`\\bdispatch_${site}\\b`);
  return rustSources(root).find((file) => {
    if (!file.startsWith('features/')) return false;
    const source = read(root, file);
    return /impl Keeper for/.test(source) && crate.test(source) && names.test(source);
  });
}
/** A feature's page as its `frontend/feature.ts` declares it: its address and label. */
export function pageOf(root: string, feature: string) {
  const file = `features/${feature}/frontend/feature.ts`;
  if (!exists(root, file)) return undefined;
  const source = read(root, file);
  // The first of its routes: a sidebar of pages it declares is none.
  const routes = source.indexOf('routes: [');
  const match = /id:\s*'([^']+)',\s*scope:\s*'dsp',\s*label:\s*'([^']+)'/.exec(
    routes < 0 ? '' : source.slice(routes),
  );
  return match
    ? { id: match[1]!, label: match[2]!, hostsSettings: /\bhostsSettings: true\b/.test(source) }
    : undefined;
}
/** Every page's id in a DSP, and every sidebar of pages', as the owners' manifests declare them. */
export function dspPages(root: string) {
  return [
    ...features(root).map((owner) => `features/${owner}`),
    ...collectors(root).map((owner) => `collectors/${owner}`),
    ...coreParts(root).map((owner) => `core/${owner}`),
  ]
    .map((dir) => `${dir}/frontend/feature.ts`)
    .filter((file) => exists(root, file))
    .flatMap((file) =>
      [...read(root, file).matchAll(/\bid:\s*'([^']+)',\s*scope:\s*'dsp'/g)].map(
        (found) => found[1]!,
      ),
    );
}

/** The first existing candidate file that holds `pattern`; fails when none does. */
export function holding(root: string, candidates: string[], pattern: RegExp) {
  const file = candidates.find(
    (candidate) => exists(root, candidate) && pattern.test(read(root, candidate)),
  );
  if (!file) throw new Error(`None of ${candidates.join(', ')} holds ${pattern}`);
  return file;
}
/** Where the app's backend lists every collector and feature. */
export const appBackend = ['app/backend/features.rs', 'app/backend/lib.rs'];
