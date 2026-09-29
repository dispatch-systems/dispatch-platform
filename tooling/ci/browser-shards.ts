import { execFileSync } from 'node:child_process';
import fs from 'node:fs';

type Suite = {
  title: string;
  file: string;
  specs?: { title: string }[];
  suites?: Suite[];
};

/** Median seconds per browser test in the merge queue, by `file › title`. */
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

/**
 * Splits `tests` into `count` shards that take about as long as each other: longest first,
 * each to the shard with the least time so far. A test without a recorded time counts as the
 * median of the rest. Every test lands in exactly one shard, and the split depends only on the
 * tests and their times, so each shard's runner computes the same one.
 */
export function shards(tests: string[], count: number, times = durations): string[][] {
  const known = Object.values(times).sort((a, b) => a - b);
  const fallback = known.length ? known[Math.floor(known.length / 2)]! : 1;
  const time = (test: string) => times[test] ?? fallback;
  const order = [...tests].sort((a, b) => time(b) - time(a) || (a < b ? -1 : a > b ? 1 : 0));
  const result = Array.from({ length: count }, () => ({ tests: [] as string[], total: 0 }));
  for (const test of order) {
    const lightest = result.reduce((best, shard) => (shard.total < best.total ? shard : best));
    lightest.tests.push(test);
    lightest.total += time(test);
  }
  return result.map((shard) => shard.tests);
}
