import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

/**
 * The Rust test build's size budget, checked after the core job's tests. Debug builds keep
 * line tables only (`[profile.dev]` in Cargo.toml), and each crate's integration tests share one
 * program: the workspace's 32 test programs and binaries took 1.36 GB together, the largest
 * 141 MB. With full debug info and a program per test file they were 55 taking 2.87 GB, each
 * up to four times its size now, and one worktree's build passed 25 GB. The budgets leave
 * headroom. Never raise one to let a run pass: find what grew.
 */
export const budgets = {
  /** The largest test program. */
  programBytes: 160 * 1024 ** 2,
  /** Every test program together. */
  totalBytes: 1.75 * 1024 ** 3,
};

export interface Program {
  name: string;
  bytes: number;
}
/** The test programs Cargo built in `deps`: executables named `<target>-<16 hex digits>`. */
export function programs(deps: string): Program[] {
  return fs
    .readdirSync(deps, { withFileTypes: true })
    .filter((entry) => entry.isFile() && /-[0-9a-f]{16}$/.test(entry.name))
    .map((entry) => ({ entry, stat: fs.statSync(path.join(deps, entry.name)) }))
    .filter(({ stat }) => (stat.mode & 0o111) !== 0)
    .map(({ entry, stat }) => ({ name: entry.name, bytes: stat.size }));
}
const megabytes = (bytes: number) => `${Math.round(bytes / 1024 ** 2)} MB`;
/** What breaks the budget: each program over its own, then the total. */
export function overBudget(found: Program[], limits = budgets): string[] {
  const problems = found
    .filter(({ bytes }) => bytes > limits.programBytes)
    .sort((a, b) => b.bytes - a.bytes)
    .map(
      ({ name, bytes }) =>
        `${name} is ${megabytes(bytes)}, over the ${megabytes(limits.programBytes)} a test program may take.`,
    );
  const total = found.reduce((sum, { bytes }) => sum + bytes, 0);
  if (total > limits.totalBytes)
    problems.push(
      `The ${found.length} test programs take ${megabytes(total)}, over the ${megabytes(limits.totalBytes)} they may take together.`,
    );
  return problems;
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(import.meta.filename)) {
  const metadata = JSON.parse(
    execFileSync('cargo', ['metadata', '--locked', '--no-deps', '--format-version=1'], {
      encoding: 'utf8',
    }),
  ) as { target_directory: string };
  const found = programs(path.join(metadata.target_directory, 'debug/deps'));
  if (!found.length) throw new Error('No test programs were built: run this after the Rust tests.');
  const problems = overBudget(found);
  const total = found.reduce((sum, { bytes }) => sum + bytes, 0);
  const largest = Math.max(...found.map(({ bytes }) => bytes));
  console.log(
    `${found.length} test programs take ${megabytes(total)}; the largest ${megabytes(largest)}.`,
  );
  if (problems.length) {
    for (const problem of problems) console.error(problem);
    process.exitCode = 1;
  }
}
