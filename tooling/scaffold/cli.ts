import path from 'node:path';
import { described as diagnosis, doctor } from './doctor.js';
import { generate, report } from './generate.js';
import { LISTS, explain, fields, table } from './inventory.js';
import { generator as collector } from './new-collector.js';
import { generator as feature } from './new-feature.js';
import { generator as slot } from './new-slot.js';
import { generator as tool } from './new-tool.js';
import {
  UsageError,
  asksUsage,
  described,
  failed,
  make,
  nextLines,
  repositoryRoot,
  type Generator,
} from './scaffold.js';

// What `dispatchdev new`, `generate`, `list`, `explain` and `doctor` run, the checkout's own copy: making
// something, writing what is generated from it, and reading what the product has. Each takes
// --json, which prints its answer as JSON on stdout and its progress on stderr.

/** Everything `dispatchdev new` makes, in the order it lists them. */
export const KINDS: Generator[] = [feature, collector, tool, slot];
/** What `new` says last once it has generated too, in place of the notes naming generate. */
const AGAIN = 'After changing what it declares, `dispatchdev generate` writes what follows again.';

const out = (text: string) => process.stdout.write(`${text}\n`);
const err = (text: string) => process.stderr.write(`${text}\n`);
/** `--root <dir>` or `--root=<dir>` taken out of the arguments, or this checkout. */
function rooted(argv: string[]) {
  const rest = [...argv];
  let root = repositoryRoot;
  const at = rest.findIndex((arg) => arg === '--root' || arg.startsWith('--root='));
  if (at >= 0) {
    const [flag] = rest.splice(at, 1) as [string];
    const value = flag.includes('=') ? flag.slice('--root='.length) : rest.splice(at, 1)[0];
    if (!value) throw new UsageError('--root needs a value');
    root = path.resolve(value);
  }
  return { root, rest };
}
const columns = (rows: [string, string][]) =>
  rows.map(([name, about]) => `  ${name.padEnd(13)} ${about}`);

type Command = { usage: () => string; run: (argv: string[], json: boolean) => Promise<boolean> };
const COMMANDS: Record<string, Command> = {
  new: {
    usage: () =>
      [
        'Usage: dispatchdev new <kind> <name> [options] [--json]',
        '',
        ...columns(KINDS.map((kind) => [kind.kind, kind.about])),
        '',
        'dispatchdev new <kind> lists its options.',
      ].join('\n'),
    run: create,
  },
  generate: {
    usage: () => 'Usage: dispatchdev generate [--json]',
    run: async (argv, json) => {
      const { root, rest } = rooted(argv);
      if (rest.length) throw new UsageError(`generate takes no ${rest[0]}`);
      const generated = await generate(root, { quiet: json, say: json ? err : out });
      out(json ? JSON.stringify(generated) : report(generated));
      return generated.ok;
    },
  },
  list: {
    usage: () =>
      [
        'Usage: dispatchdev list <what> [--json]',
        '',
        ...columns(Object.entries(LISTS).map(([name, list]) => [name, list.about])),
      ].join('\n'),
    run: async (argv, json) => {
      const { root, rest } = rooted(argv);
      const [what, extra] = rest;
      if (!what) {
        out(COMMANDS.list!.usage());
        return true;
      }
      const list = LISTS[what];
      if (!list || extra)
        throw new UsageError(`dispatchdev list lists one of ${Object.keys(LISTS).join(', ')}`);
      const rows = list.rows(root);
      out(json ? JSON.stringify(rows) : table(rows));
      return true;
    },
  },
  doctor: {
    usage: () => 'Usage: dispatchdev doctor [--json]',
    run: async (argv, json) => {
      const { root, rest } = rooted(argv);
      if (rest.length) throw new UsageError(`doctor takes no ${rest[0]}`);
      const findings = doctor(root);
      out(json ? JSON.stringify(findings) : diagnosis(findings));
      return findings.every((finding) => finding.ok);
    },
  },
  explain: {
    usage: () => 'Usage: dispatchdev explain <feature | collector> [--json]',
    run: async (argv, json) => {
      const { root, rest } = rooted(argv);
      if (!rest.length) {
        out(COMMANDS.explain!.usage());
        return true;
      }
      if (rest.length !== 1) throw new UsageError('Name one feature or collector');
      const parts = explain(root, rest[0]!);
      out(json ? JSON.stringify(parts) : fields(parts));
      return true;
    },
  },
};

/** `new <kind> <name> [options]`: makes it, then writes what is generated from it. */
async function create(argv: string[], json: boolean) {
  const [name, ...rest] = argv;
  if (!name || name === '--help' || name === '-h') {
    out(COMMANDS.new!.usage());
    return true;
  }
  const kind = KINDS.find((each) => each.kind === name);
  if (!kind)
    throw new UsageError(`dispatchdev new makes ${KINDS.map((each) => each.kind).join(', ')}`);
  if (asksUsage(rest)) {
    out(kind.usage);
    return true;
  }
  let made;
  try {
    made = await make(kind, rest);
  } catch (error) {
    err(failed(error, kind.usage).trimEnd());
    return false;
  }
  const generates = kind.generates && !made.dryRun;
  if (!generates) {
    out(json ? JSON.stringify({ made }) : described(made));
    return true;
  }
  if (!json) out(described(made, false));
  const generated = await generate(made.root, { quiet: json, say: json ? err : out });
  const next = [...made.next.filter((note) => !note.includes('`dispatchdev generate`')), AGAIN];
  if (json) out(JSON.stringify({ made: { ...made, next }, generated }));
  else out([report(generated), ...(generated.ok ? nextLines(next) : [])].join('\n'));
  return generated.ok;
}

export async function main(argv: string[]) {
  const [name, ...rest] = argv;
  const command = COMMANDS[name ?? ''];
  if (!command) throw new Error(`Run as dispatchdev ${Object.keys(COMMANDS).join(' | ')}`);
  const json = rest.includes('--json');
  const args = rest.filter((arg) => arg !== '--json');
  // `new <kind> --help` is the kind's usage, which `create` answers.
  if (name !== 'new' && (args.includes('--help') || args.includes('-h'))) {
    out(command.usage());
    return true;
  }
  try {
    return await command.run(args, json);
  } catch (error) {
    err(failed(error, command.usage()).trimEnd());
    return false;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(import.meta.filename))
  if (!(await main(process.argv.slice(2)))) process.exitCode = 1;
