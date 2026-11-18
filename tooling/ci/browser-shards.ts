import { execFileSync } from 'node:child_process';
import fs from 'node:fs';

type Suite = {
  title: string;
  file: string;
  specs?: { title: string }[];
  suites?: Suite[];
};

/** The workers a shard runs its tests on. */
export const WORKERS = 3;

/** Median seconds per browser test in main's runs of the checks, by `file › title`. */
const durations: Record<string, number> = JSON.parse(
  fs.readFileSync(new URL('./browser-durations.json', import.meta.url), 'utf8'),
);

/**
 * Every browser test as `file › describe › title`, the form `--test-list` takes, from
 * Playwright's own listing of the suite.
 */
export function browserTests(): string[] {
  const listing = JSON.parse(
    execFileSync(
      process.execPath,
      ['node_modules/@playwright/test/cli.js', 'test', '--list', '--reporter=json'],
      { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 },
    ),
  ) as { suites: Suite[] };
  const tests: string[] = [];
  const walk = (suite: Suite, path: string[]) => {
    for (const spec of suite.specs ?? []) tests.push([suite.file, ...path, spec.title].join(' › '));
    for (const child of suite.suites ?? []) walk(child, [...path, child.title]);
  };
  for (const file of listing.suites) walk(file, []);
  return tests;
}

/** `tests` by their spec file, each named `file › … › title`. */
function byFile(tests: string[]): Map<string, string[]> {
  const files = new Map<string, string[]>();
  for (const test of tests) {
    const file = test.slice(0, test.indexOf(' › '));
    files.set(file, [...(files.get(file) ?? []), test]);
  }
  return files;
}

/**
 * The recorded time of each of `tests` that has one. A renamed test keeps the time recorded
 * under its old title: when a file has one test without a time and one recorded title the
 * suite no longer lists, that time is the test's.
 */
function recorded(tests: string[], times: Record<string, number>): Map<string, number> {
  const found = new Map(
    tests.filter((test) => Object.hasOwn(times, test)).map((test) => [test, times[test]!]),
  );
  const listed = new Set(tests);
  const stale = byFile(Object.keys(times).filter((title) => !listed.has(title)));
  for (const [file, [test, ...others]] of byFile(tests.filter((test) => !found.has(test)))) {
    const old = stale.get(file);
    if (!others.length && old?.length === 1) found.set(test!, times[old[0]!]!);
  }
  return found;
}

/** The tests among `tests` with no recorded time, under their title or the one they had. */
export function untimed(tests: string[], times = durations): string[] {
  const found = recorded(tests, times);
  return tests.filter((test) => !found.has(test));
}

/**
 * Splits `tests` into `count` shards that take about as long as each other: longest first,
 * each to the shard with the least time so far. A test with no recorded time (`untimed`)
 * counts as the median of the recorded ones, so a new test only costs the split some balance.
 * Every test lands in exactly one shard, and the split depends only on the tests and
 * their times, so each shard's runner computes the same one.
 */
export function shards(tests: string[], count: number, times = durations): string[][] {
  const known = Object.values(times).sort((a, b) => a - b);
  const fallback = known.length ? known[Math.floor(known.length / 2)]! : 1;
  const found = recorded(tests, times);
  const time = (test: string) => found.get(test) ?? fallback;
  const order = [...tests].sort((a, b) => time(b) - time(a) || (a < b ? -1 : a > b ? 1 : 0));
  const result = Array.from({ length: count }, () => ({ tests: [] as string[], total: 0 }));
  for (const test of order) {
    const lightest = result.reduce((best, shard) => (shard.total < best.total ? shard : best));
    lightest.tests.push(test);
    lightest.total += time(test);
  }
  return result.map((shard) => shard.tests);
}
