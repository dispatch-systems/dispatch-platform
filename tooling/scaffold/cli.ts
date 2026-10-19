import path from 'node:path';
import { generator as piece } from './add.js';
import { described as diagnosis, doctor } from './doctor.js';
import { followUp, generate, report } from './generate.js';
import { LISTS, explain, fields, table } from './inventory.js';
import { generator as collector } from './new-collector.js';
import { generator as feature } from './new-feature.js';
import { generator as integration } from './new-integration.js';
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
import { known, leftLines, listed, named, todos } from './todo.js';

// What `dispatchdev new`, `add`, `generate`, `list`, `explain`, `doctor` and `todo` run, the
// checkout's own copy: making something, writing what is generated from it, and reading what the
// product has and what is left to write. Each takes --json, which prints its answer as JSON on
// stdout and its progress on stderr.

/** Everything `dispatchdev new` makes, in the order it lists them. */
export const KINDS: Generator[] = [feature, collector, integration, tool, slot];
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
  add: {
    usage: () => piece.usage,
    run: (argv, json) => makeThenGenerate(piece, argv, json),
  },
  generate: {
    usage: () =>
      [
        'Usage: dispatchdev generate [--check] [--json]',
        '',
        '  --check   write nothing, and fail naming what it would change',
      ].join('\n'),
    run: async (argv, json) => {
      const { root, rest } = rooted(argv);
      const check = rest.includes('--check');
      const extra = rest.find((arg) => arg !== '--check');
      if (extra) throw new UsageError(`generate takes no ${extra}`);
      const generated = await generate(root, { quiet: json, say: json ? err : out, check });
      const next = followUp(root, generated, check);
      if (json) out(JSON.stringify({ ...generated, next }));
      else out([report(generated, check), ...(next.length ? nextLines(next) : [])].join('\n'));
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
  todo: {
    usage: () =>
      [
        'Usage: dispatchdev todo [<owner>] [--json]',
        '',
        "What the generators left to write, by owner, or only <owner>'s: a name, as parking, or",
        'a kind and a name, as feature:parking, integration:<name>, core:<part> or mcp.',
      ].join('\n'),
    run: async (argv, json) => {
      const { root, rest } = rooted(argv);
      const [owner, extra] = rest;
      if (extra || owner?.startsWith('-')) throw new UsageError(`todo takes no ${extra ?? owner}`);
      if (owner && !known(root, owner)) throw new UsageError(`No owner is named ${owner}`);
      const owned = todos(root).filter((each) => !owner || named(owner, each.owner));
      const nothing = `Nothing is left to write${owner ? ` in ${owner}` : ''}.`;
      out(json ? JSON.stringify(owned) : owned.length ? listed(owned) : nothing);
      return true;
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
  return makeThenGenerate(kind, rest, json);
}

/** Makes what `argv` asks of `kind`, then writes what is generated from it. */
async function makeThenGenerate(kind: Generator, argv: string[], json: boolean) {
  let made;
  try {
    made = await make(kind, argv);
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
  else {
    const after = generated.ok ? [...nextLines(next), ...leftLines(made.todos)] : [];
    out([report(generated), ...after].join('\n'));
  }
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
