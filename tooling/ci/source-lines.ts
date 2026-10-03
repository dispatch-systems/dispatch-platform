import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/** Existing raw markup/provider scripts: wrapping one must lower its budget. */
export const lineExceptions: ReadonlyMap<string, number> = new Map([
  ['core/server/backend/mail/templates.rs', 4],
  ['collectors/paycom/probes/benchmark.rs', 2],
  ['collectors/paycom/collections/timecards/collect.rs', 3],
]);
/** Where the product's Rust lives, relative to the repository root. */
export const rustSourceRoots = ['app', 'core', 'collectors', 'features'];

export function sourceLineViolations(
  root = '.',
  exceptions: ReadonlyMap<string, number> = lineExceptions,
  roots?: readonly string[],
): string[] {
  const long = new Map<string, number[]>();
  const names = roots
    ? roots
        .filter((dir) => fs.existsSync(path.join(root, dir)))
        .flatMap((dir) =>
          fs
            .readdirSync(path.join(root, dir), { recursive: true, encoding: 'utf8' })
            .map((name) => path.join(dir, name)),
        )
    : fs.readdirSync(root, { recursive: true, encoding: 'utf8' });
  for (const name of names) {
    // Integration tests and their support were never held to it; module tests are.
    if (!name.endsWith('.rs') || /(^|\/)tests\/(backend\/integration|support)\//.test(name))
      continue;
    const lines = fs.readFileSync(path.join(root, name), 'utf8').split('\n');
    const offending = lines.flatMap((line, index) =>
      // Unicode code points, excluding the CR in Windows line endings.
      [...line.replace(/\r$/, '')].length > 140 ? [index + 1] : [],
    );
    if (offending.length) long.set(name, offending);
  }
  const problems: string[] = [];
  for (const [file, expected] of exceptions) {
    const found = long.get(file)?.length ?? 0;
    if (found !== expected)
      problems.push(
        `${file}: expected ${expected} long-line exceptions, found ${found}; lower the budget when wrapping, never increase it`,
      );
    long.delete(file);
  }
  for (const [file, lines] of long)
    for (const line of lines)
      problems.push(`${file}:${line}: source lines must fit in 140 characters`);
  return problems;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const problems = sourceLineViolations('.', lineExceptions, rustSourceRoots);
  if (problems.length) {
    process.stderr.write(`${problems.join('\n')}\n`);
    process.exitCode = 1;
  } else process.stdout.write('Backend source-line policy passes.\n');
}
