import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/** Existing raw markup/provider scripts: wrapping one must lower its budget. */
export const lineExceptions: ReadonlyMap<string, number> = new Map([
  ['mail/templates.rs', 4],
  ['browsers/paycom/benchmark.rs', 2],
  ['browsers/paycom/collection.rs', 3],
]);

export function sourceLineViolations(
  root = 'backend/src',
  exceptions: ReadonlyMap<string, number> = lineExceptions,
): string[] {
  const long = new Map<string, number[]>();
  for (const name of fs.readdirSync(root, { recursive: true, encoding: 'utf8' })) {
    if (!name.endsWith('.rs')) continue;
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
  const problems = sourceLineViolations();
  if (problems.length) {
    process.stderr.write(`${problems.join('\n')}\n`);
    process.exitCode = 1;
  } else process.stdout.write('Backend source-line policy passes.\n');
}
