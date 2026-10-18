import path from 'node:path';
import {
  UsageError,
  change,
  emptyPlan,
  exists,
  formatRust,
  names,
  parseArguments,
  read,
  run,
  template,
  type Generator,
  type Plan,
} from './scaffold.js';

// `dispatchdev new slot <name> [options]`: a new kind of slot, something a feature declares in
// its manifest and a host of core's reads. It writes the slot's backend module, with the
// declaration, what the registry answers of it and its check, and its frontend module, with
// what an owner puts in it and what a host reads it with; adds the slot to `Feature`,
// `feature()`, `Registry::check` and the `pub use` list, re-exports its names from `slots.ts`,
// and gives it its rows in core/manifest/README.md, which the catalogue rule holds. What's
// written works as written; the host that reads it is the work left.

export const usage = `Usage: dispatchdev new slot <name> [options]

  --description <text>    what a feature declares in it, in one sentence of at most 160
                          characters: "Its badges on the DSPs page."
  --each <name>           one of them, as the builder and the type are named (default: the
                          name without its final s, as badges is badge)
  --backend-only          the backend module alone, for what no page draws
  --frontend-only         the frontend module alone, for what the backend never reads
  --dry-run               print what it would write and change nothing
  --out <dir>             with --dry-run, write the files under <dir> instead
  --root <dir>            the repository to write into (default: this one)

Name it for what a feature declares several of, as badges or shortcuts.`;

const FLAGS = ['backend-only', 'frontend-only', 'dry-run'];
const OPTIONS = ['description', 'each', 'out', 'root'];
const MANIFEST = 'core/manifest/backend/mod.rs';
const SLOTS = 'core/shell/frontend/runtime/slots.ts';
const CATALOGUE = 'core/manifest/README.md';

/** The slot's names: the field, one of them, and how each reads in Rust and TypeScript. */
export function slotNames(root: string, argv: string[]) {
  const args = parseArguments(argv, FLAGS, OPTIONS);
  if (args.positional.length !== 1) throw new UsageError('Name the slot, and only the slot');
  const [name] = args.positional as [string];
  const slot = names(name);
  const singular = args.options.get('each') ?? (/[^s]s$/.test(name) ? name.slice(0, -1) : name);
  const each = names(singular);
  if (args.flags.has('backend-only') && args.flags.has('frontend-only'))
    throw new UsageError('--backend-only and --frontend-only leave nothing to write');
  const backend = !args.flags.has('frontend-only');
  const frontend = !args.flags.has('backend-only');
  const description = (
    args.options.get('description') ?? `What a feature declares as its ${slot.label.toLowerCase()}.`
  ).trim();
  if (!/[.!?]$/.test(description))
    throw new UsageError('--description is a sentence, ending with a full stop');
  if (description.length > 160)
    throw new UsageError('--description is one sentence, 160 characters at most');
  if (backend) {
    if (exists(root, `core/manifest/backend/${name}.rs`))
      throw new UsageError(`${name} is taken: core/manifest/backend/${name}.rs holds it`);
    if (new RegExp(`^\\s+pub ${name}:`, 'm').test(read(root, MANIFEST)))
      throw new UsageError(`${name} is taken: Feature has a field of that name`);
  }
  if (frontend && exists(root, `core/shell/frontend/runtime/slots/${slot.slug}.ts`))
    throw new UsageError(`${name} is taken: runtime/slots/${slot.slug}.ts holds it`);
  return { args, slot, each, backend, frontend, description };
}

/** Inserts `line` among the lines `pattern` matches, in their order; after the last, if none sorts later. */
function amongSorted(text: string, pattern: RegExp, line: string, file: string) {
  const lines = text.split('\n');
  const indices = lines.flatMap((each, index) => (pattern.test(each) ? [index] : []));
  if (!indices.length) throw new Error(`${file}: cannot find ${pattern}; add ${line} by hand`);
  const at = indices.find((index) => lines[index]! > line) ?? indices.at(-1)! + 1;
  lines.splice(at, 0, line);
  return lines.join('\n');
}
/** Inserts `addition` before `anchor`, which must appear once. */
function before(text: string, anchor: string, addition: string, file: string) {
  const at = text.indexOf(anchor);
  if (at < 0 || text.indexOf(anchor, at + 1) >= 0)
    throw new Error(`${file}: cannot place a line before ${anchor.trim()}; add it by hand`);
  return `${text.slice(0, at)}${addition}${text.slice(at)}`;
}
/** Inserts `row` after the last row of the table `heading` opens. */
function tableRow(text: string, heading: string, row: string, file: string) {
  const start = text.indexOf(heading);
  if (start < 0) throw new Error(`${file}: cannot find ${heading}; add the row by hand`);
  const rest = text.slice(start);
  const end = rest.search(/\n(?!\|)[^\n]*\n|\n$(?![\s\S])/);
  const tableEnd = end < 0 ? text.length : start + end;
  // The table's last row ends at the newline before the first line that isn't a row.
  const lastRow = text.lastIndexOf('\n|', tableEnd);
  const lineEnd = text.indexOf('\n', lastRow + 1);
  return `${text.slice(0, lineEnd)}\n${row}${text.slice(lineEnd)}`;
}

export async function planSlot(root: string, argv: string[]) {
  const { args, slot, each, backend, frontend, description } = slotNames(root, argv);
  const plan: Plan = emptyPlan();
  const values = {
    name: slot.name,
    pascal: each.pascal,
    singular: each.name,
    camel: slot.camel,
    description,
  };
  if (backend) {
    plan.files.set(`core/manifest/backend/${slot.name}.rs`, template('slot/backend.rs', values));
    await change(plan, root, MANIFEST, (text) => {
      let next = amongSorted(text, /^pub mod \w+;$/, `pub mod ${slot.name};`, MANIFEST);
      next = amongSorted(
        next,
        /^pub use \w+::/,
        `pub use ${slot.name}::{${each.pascal}, ${each.name}};`,
        MANIFEST,
      );
      // The last field and its empty value, after `services`.
      next = before(next, '    pub services: Services,\n}', '', MANIFEST).replace(
        '    pub services: Services,\n}',
        `    pub services: Services,\n    /// ${description}\n    pub ${slot.name}: &'static [${each.pascal}],\n}`,
      );
      next = next.replace(
        '        services: Services::NONE,\n    }',
        `        services: Services::NONE,\n        ${slot.name}: &[],\n    }`,
      );
      return before(
        next,
        '        agents::check(self);',
        `        ${slot.name}::check(self);\n`,
        MANIFEST,
      );
    });
    await change(plan, root, CATALOGUE, (text) =>
      tableRow(
        text,
        '## Backend slots',
        `| \`${slot.name}\` | \`${slot.name}\` | ${description} |`,
        CATALOGUE,
      ),
    );
  }
  if (frontend) {
    plan.files.set(
      `core/shell/frontend/runtime/slots/${slot.slug}.ts`,
      template('slot/frontend.ts', values),
    );
    await change(
      plan,
      root,
      SLOTS,
      (text) =>
        `${text.trimEnd()}\nexport type { ${each.pascal} } from './slots/${slot.slug}.js';\nexport { ${slot.camel} } from './slots/${slot.slug}.js';\n`,
    );
    await change(plan, root, CATALOGUE, (text) =>
      tableRow(
        text,
        '## Frontend slots',
        `| \`${slot.camel}\` | \`${slot.slug}\` | ${description} |`,
        CATALOGUE,
      ),
    );
  }
  formatRust(plan);
  if (backend)
    plan.notes.push(
      `Give the slot what its host needs: ${each.pascal} in core/manifest/backend/${slot.name}.rs, and what \`check\` holds every declaration to.`,
    );
  if (frontend)
    plan.notes.push(
      `Have a host draw it: what \`${slot.camel}(view)\` answers, from core/shell/frontend/runtime/slots/${slot.slug}.ts.`,
    );
  plan.notes.push(
    'Its rows in core/manifest/README.md say what it adds; app/tests/rules/catalogue.test.ts holds them there.',
  );
  return { plan, args };
}

export const generator: Generator = {
  kind: 'slot',
  about: 'a new kind of slot in core: what a feature declares in its manifest and a host reads',
  usage,
  flags: FLAGS,
  options: OPTIONS,
  plan: planSlot,
  generates: false,
};

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(import.meta.filename))
  await run(generator);
