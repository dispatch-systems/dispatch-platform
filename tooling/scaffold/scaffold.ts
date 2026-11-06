import path from 'node:path';
import { UsageError, parseArguments, repositoryRoot, type Arguments } from './common.js';
import { described, finish, type Made, type Plan } from './plan.js';

/** A kind of thing `dispatchdev new` makes: its usage, its options and how it plans one. */
export type Generator = {
  kind: string;
  /** What it makes, in a line. */
  about: string;
  usage: string;
  flags: string[];
  options: string[];
  plan: (root: string, argv: string[]) => Promise<{ plan: Plan; args: Arguments }>;
  /** Whether `dispatchdev generate` runs once it has written. */
  generates: boolean;
};
/** Whether the arguments only ask for the usage: none, or --help. */
export const asksUsage = (argv: string[]) =>
  argv.length === 0 || argv.includes('--help') || argv.includes('-h');
/** Plans what `argv` asks of the generator and writes it, or with --dry-run only says so. */
export async function make(generator: Generator, argv: string[]): Promise<Made> {
  const options = parseArguments(argv, generator.flags, generator.options);
  const root = path.resolve(options.options.get('root') ?? repositoryRoot);
  const { plan, args } = await generator.plan(root, argv);
  const dryRun = args.flags.has('dry-run');
  if (args.options.has('out') && !dryRun) throw new UsageError('--out goes with --dry-run');
  return finish(plan, root, { dryRun, out: args.options.get('out') });
}
/** The message of a failure, with the usage after a usage error. */
export const failed = (error: unknown, usage: string) =>
  `${error instanceof Error ? error.message : String(error)}\n${error instanceof UsageError ? `\n${usage}\n` : ''}`;

/**
 * Runs a generator as its own script: the usage, or what it made, as JSON with --json, or why it
 * failed.
 */
export async function run(generator: Generator) {
  const argv = process.argv.slice(2).filter((arg) => arg !== '--json');
  const json = argv.length < process.argv.length - 2;
  try {
    if (asksUsage(argv)) process.stdout.write(`${generator.usage}\n`);
    else {
      const made = await make(generator, argv);
      process.stdout.write(`${json ? JSON.stringify({ made }) : described(made)}\n`);
    }
  } catch (error) {
    process.stderr.write(failed(error, generator.usage));
    process.exitCode = 1;
  }
}
