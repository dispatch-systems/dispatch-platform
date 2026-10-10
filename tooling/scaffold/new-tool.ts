import path from 'node:path';
import {
  UsageError,
  appendToList,
  change,
  emptyPlan,
  features,
  files,
  finish,
  formatRust,
  parseArguments,
  read,
  repositoryRoot,
  run,
  snapshotsNote,
  template,
  type Plan,
  type Values,
} from './scaffold.js';

// `dispatchdev new tool <name> [options]`: a tool agents can use, in mcp/tools/: its file,
// which works as written, a test that calls it as an agent does, and its place in
// `tools::TOOLS`. Who may use it is no part of it: the platform owner chooses, for each key and
// app, on the Agents page: Off, Read, or Read and change.

export const usage = `Usage: npm run new:tool -- <name> [options]

  --scope <scope>         what it is about: dsp, one DSP a call names (the default); dsps,
                          several, those a call names or every one the connection reaches;
                          or connection, the connection itself
  --features <ids>        the features, or their parts, it needs, by their switches, as
                          timecard,routes or timecard.meal_breaks: it runs at a DSP only
                          where every one is on
  --actions <names>       what it can be asked to do, as list,approve: one tool doing several
                          things, each action with arguments of its own
  --changes               every call changes something
  --changing <names>      with --actions, the actions that change something
  --title <title>         its name as people read it (default: from <name>)
  --description <text>    what it does and answers, for the model choosing a tool
  --dry-run               print what it would write and change nothing
  --out <dir>             with --dry-run, write the files under <dir> instead
  --root <dir>            the repository to write into (default: this one)

A key or app reads with it at most until the platform owner allows it to change something.`;

const FLAGS = ['changes', 'dry-run'];
const OPTIONS = ['scope', 'features', 'actions', 'changing', 'title', 'description', 'out', 'root'];
const SCOPES = ['dsp', 'dsps', 'connection'] as const;
type Scope = (typeof SCOPES)[number];
/** Where the tools live, and the list that names them. */
const TOOLS = 'mcp/tools';
const TOOLS_LIST = /pub const TOOLS: &\[&dyn AnyTool\] = &\[/;
const WORD = /^[a-z][a-z0-9]*(_[a-z0-9]+)*$/;

/** Every tool name already taken, with the file that declares it. */
function toolNames(root: string) {
  const declared = new Map<string, string>();
  for (const file of files(root, TOOLS, /\.rs$/))
    for (const match of read(root, file).matchAll(/const NAME: &'static str = "([^"]+)";/g))
      declared.set(match[1]!, file);
  return declared;
}
/** Every switch a tool can need: each feature's, and each of its parts'. */
function switches(root: string) {
  const found = new Set<string>();
  for (const owner of features(root)) {
    const manifest = read(root, `features/${owner}/feature.rs`);
    for (const match of manifest.matchAll(/(?:optional|mandatory)\(\s*"([^"]+)"/g))
      found.add(match[1]!);
    for (const match of manifest.matchAll(/\b(?:tab|sub)\(\s*"([^"]+)"/g)) found.add(match[1]!);
  }
  return found;
}
/** A comma-separated list of names, each one `pattern` takes, in the order given. */
function listOf(value: string | undefined, option: string, pattern: RegExp) {
  if (value === undefined) return [];
  const listed = value
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean);
  if (!listed.length) throw new UsageError(`--${option} names nothing`);
  for (const item of listed)
    if (!pattern.test(item)) throw new UsageError(`${item} is no name --${option} takes`);
  if (new Set(listed).size !== listed.length)
    throw new UsageError(`--${option} names one more than once`);
  return listed;
}
/** A Rust string literal's inside: `"` and `\` escaped. */
const literal = (text: string) => text.replace(/[\\"]/g, (c) => `\\${c}`);
/** `text` as doc comment lines of at most `width` characters after `prefix`. */
function wrapped(text: string, prefix: string, width = 92) {
  const lines: string[] = [];
  let line = '';
  for (const word of text.split(/\s+/).filter(Boolean)) {
    if (line && line.length + 1 + word.length > width) {
      lines.push(line);
      line = word;
    } else line = line ? `${line} ${word}` : word;
  }
  if (line) lines.push(line);
  return lines.map((each) => `${prefix}${each}`).join('\n');
}
/** A name's words, each capitalized: `approve_timecard` is `ApproveTimecard`. */
const pascalOf = (name: string) =>
  name
    .split('_')
    .map((word) => word[0]!.toUpperCase() + word.slice(1))
    .join('');
/** A name as people read it: `approve_timecard` is `Approve timecard`. */
const labelOf = (name: string) => {
  const words = name.split('_');
  return [words[0]![0]!.toUpperCase() + words[0]!.slice(1), ...words.slice(1)].join(' ');
};

export function toolValues(root: string, argv: string[]) {
  const args = parseArguments(argv, FLAGS, OPTIONS);
  if (args.positional.length !== 1) throw new UsageError('Name the tool, and only the tool');
  const [name] = args.positional as [string];
  if (!WORD.test(name) || name.length > 64)
    throw new UsageError(
      `${name} is not a tool's name: lowercase words joined by _, as approve_timecard`,
    );
  const taken = toolNames(root).get(name);
  if (taken) throw new UsageError(`${name} is taken: ${taken} declares it`);
  const scope = (args.options.get('scope') ?? 'dsp') as Scope;
  if (!SCOPES.includes(scope))
    throw new UsageError(`--scope is ${SCOPES.join(', ')}, not ${args.options.get('scope')}`);
  const needed = listOf(args.options.get('features'), 'features', /^[a-z][a-z0-9_.]*$/);
  const known = switches(root);
  for (const id of needed)
    if (!known.has(id))
      throw new UsageError(`${id} is no feature or part of one: no manifest declares its switch`);
  const actions = listOf(args.options.get('actions'), 'actions', WORD);
  const changing = listOf(args.options.get('changing'), 'changing', WORD);
  const changes = args.flags.has('changes');
  if (changing.length && !actions.length)
    throw new UsageError('--changing names actions: give them with --actions');
  for (const action of changing)
    if (!actions.includes(action))
      throw new UsageError(`${action} is none of its --actions: ${actions.join(', ')}`);
  if (changes && actions.length)
    throw new UsageError('With --actions, name the actions that change something with --changing');
  if (scope === 'connection' && (needed.length || changes || changing.length))
    throw new UsageError(
      'A tool about the connection only reads, as every connection may use it, and needs no feature',
    );
  const title = args.options.get('title') ?? labelOf(name);
  const about = { dsp: 'the DSP', dsps: 'the DSPs', connection: 'this connection' }[scope];
  const description = args.options.get('description') ?? `Answer with ${about}.`;
  if (!/[.!?]$/.test(description.trim()))
    throw new UsageError('--description is a sentence or two, ending with a full stop');
  const values: Values = {
    name,
    pascal: pascalOf(name),
    title: literal(title),
    description: literal(description.trim()),
    dsp: scope === 'dsp',
    dsps: scope === 'dsps',
    connection: scope === 'connection',
    features: needed.length > 0,
    featureList: needed.map((id) => `"${id}"`).join(', '),
    // Each feature switched on, before its parts.
    switchOn: [...new Set(needed.flatMap((id) => [id.split('.')[0]!, id]))]
      .map((id) => `    db.set_feature(&dsp, "${id}", true, &owner).unwrap();`)
      .join('\n'),
    firstFeature: needed[0] ? needed[0].split('.')[0]! : '',
    actions: actions.length > 0,
    firstAction: actions[0] ?? '',
    changes: changes || changing.length > 0,
    reads: scope !== 'connection' && !changes && !changing.length,
    refusals: changes || changing.length > 0 || needed.length > 0,
    everyCall: changes,
    changingAction: changing[0] ?? '',
    reading: actions.find((action) => !changing.includes(action)) ?? '',
  };
  return {
    args,
    values,
    docs: wrapped(description.trim(), '/// '),
    actions,
    changing,
  };
}

/** The actions' enum, what each does, and how the tool answers each. */
function actionCode(pascal: string, actions: string[], changing: string[]) {
  const variant = (action: string) => pascalOf(action);
  const declared = actions
    .map(
      (action) =>
        `    /// ${labelOf(action)}: its arguments are its fields.\n    ${variant(action)} {},`,
    )
    .join('\n');
  const effects = changing.length
    ? `    fn effect(input: &${pascal}Action) -> Effect {
        match input {
${changing.map((action) => `            ${pascal}Action::${variant(action)} {} => Effect::Changes,`).join('\n')}
${changing.length < actions.length ? '            _ => Effect::Reads,\n' : ''}        }
    }
`
    : '';
  const arms = actions
    .map((action) =>
      changing.includes(action)
        ? `            ${pascal}Action::${variant(action)} {} => {
                // Make the change here, and record it with \`w.audit\`.
                cx.write(move |_w| Ok(())).await?;
            }`
        : `            ${pascal}Action::${variant(action)} {} => {}`,
    )
    .join('\n');
  return { declared, effects, arms };
}

/** `text` with `mod <name>;` among its other modules, in the order rustfmt keeps them. */
function withModule(text: string, name: string) {
  const mods = [...text.matchAll(/^mod ([a-z_0-9]+);$/gm)];
  if (!mods.length) throw new Error(`no module list to add mod ${name}; to`);
  const before = mods.find((mod) => mod[1]! > name);
  if (before) return `${text.slice(0, before.index)}mod ${name};\n${text.slice(before.index)}`;
  const last = mods.at(-1)!;
  const at = last.index + last[0].length;
  return `${text.slice(0, at)}\nmod ${name};${text.slice(at)}`;
}

export async function planTool(root: string, argv: string[]) {
  const { args, values, docs, actions, changing } = toolValues(root, argv);
  const { name, pascal } = values as { name: string; pascal: string };
  const plan: Plan = emptyPlan();
  const code = actionCode(pascal, actions, changing);
  const tool = template('tool/tool.rs', { ...values, ...code })
    .replace(`/// ${values.description}\npub struct`, `${docs}\npub struct`)
    .replace(`//! ${values.title}: ${values.description}`, () =>
      wrapped(`${values.title}: ${values.description}`, '//! ', 96),
    );
  plan.files.set(`${TOOLS}/${name}.rs`, tool);
  plan.files.set(`mcp/tests/backend/tools/${name}.rs`, template('tool/test.rs', values));
  await change(plan, root, `${TOOLS}/mod.rs`, (text) =>
    appendToList(withModule(text, name), TOOLS_LIST, `&${name}::${pascal},`, `${TOOLS}/mod.rs`),
  );
  formatRust(plan);
  plan.notes.push(
    `Make it do what it's for: its Input and Output in ${TOOLS}/${name}.rs, and its test.`,
  );
  plan.notes.push(
    'A change it makes is recorded with `w.audit`, and its action worded in mcp/frontend/audit-wording.ts.',
  );
  plan.notes.push(snapshotsNote);
  return { plan, args };
}

export async function main(argv: string[]) {
  const options = parseArguments(argv, FLAGS, OPTIONS);
  const root = path.resolve(options.options.get('root') ?? repositoryRoot);
  const { plan, args } = await planTool(root, argv);
  const dryRun = args.flags.has('dry-run');
  if (args.options.has('out') && !dryRun) throw new UsageError('--out goes with --dry-run');
  finish(plan, root, { dryRun, out: args.options.get('out') });
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(import.meta.filename))
  await run(usage, main);
