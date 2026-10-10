import path from 'node:path';
import {
  UsageError,
  appendToList,
  change,
  emptyPlan,
  exists,
  features,
  files,
  finish,
  formatRust,
  names as namesOf,
  parseArguments,
  read,
  repositoryRoot,
  run,
  snapshotsNote,
  template,
  type Plan,
  type Values,
} from './scaffold.js';

// `dispatchdev new tool <feature> <name> [options]`: a tool agents can use, in a feature's
// mcp/: its file, which works as written, a test that calls it as an agent does, and the
// manifest's `tools` listing it. Who may use it is no part of it: the platform owner chooses,
// for each key and app, on the Agents page.

export const usage = `Usage: npm run new:tool -- <feature> <name> [options]

  --changes               it changes something, rather than only reading: no key or app uses
                          it until the platform owner switches it on for that one
  --part <part>           the part of the feature it belongs to, as timecard.meals: it is
                          offered only where that part is on too
  --title <title>         its name as people read it (default: from <name>)
  --description <text>    what it does and answers, for the model choosing a tool
  --dry-run               print what it would write and change nothing
  --out <dir>             with --dry-run, write the files under <dir> instead
  --root <dir>            the repository to write into (default: this one)`;

const FLAGS = ['changes', 'dry-run'];
const OPTIONS = ['part', 'title', 'description', 'out', 'root'];
/** The names core's own tools take, which no feature's may. */
const CORE_TOOLS = ['get_profile', 'whoami'];
/** What a tool is listed in, in its feature's mcp/mod.rs. */
const TOOLS_LIST = /pub\(crate\) const TOOLS: &\[&dyn AnyTool\] = &\[/;

/** Every tool name a feature already declares, with the file that declares it. */
function toolNames(root: string) {
  const declared = new Map<string, string>(CORE_TOOLS.map((name) => [name, 'core']));
  for (const owner of features(root))
    for (const file of files(root, `features/${owner}/mcp`, /\.rs$/))
      for (const match of read(root, file).matchAll(/const NAME: &'static str = "([^"]+)";/g))
        declared.set(match[1]!, file);
  return declared;
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
/** The crates a feature's tests install beside it: its collectors and the features it uses. */
function installed(root: string, feature: string) {
  const manifest = read(root, `features/${feature}/Cargo.toml`);
  const dependencies = manifest.split(/^\[dev-dependencies\]/m)[0]!;
  const ident = (name: string) => `dispatch_${name.replaceAll('-', '_')}`;
  const collectors = [...dependencies.matchAll(/path = "\.\.\/\.\.\/collectors\/([a-z_]+)"/g)].map(
    (match) => `&${ident(match[1]!)}::COLLECTOR`,
  );
  const used = [...dependencies.matchAll(/path = "\.\.\/([a-z_]+)"/g)].map(
    (match) => `&${ident(match[1]!)}::FEATURE`,
  );
  return { collectors: collectors.join(', '), features: [...used, '&crate::FEATURE'].join(', ') };
}

export function toolValues(root: string, argv: string[]) {
  const args = parseArguments(argv, FLAGS, OPTIONS);
  if (args.positional.length !== 2) throw new UsageError('Name the feature, then the tool');
  const [feature, name] = args.positional as [string, string];
  if (!exists(root, `features/${feature}/feature.rs`))
    throw new UsageError(`${feature} is no feature: features/${feature}/feature.rs is missing`);
  if (!/^[a-z][a-z0-9]*(_[a-z0-9]+)*$/.test(name) || name.length > 64)
    throw new UsageError(
      `${name} is not a tool's name: lowercase words joined by _, as approve_timecard`,
    );
  const taken = toolNames(root).get(name);
  if (taken) throw new UsageError(`${name} is taken: ${taken} declares it`);
  const manifest = read(root, `features/${feature}/feature.rs`);
  const part = args.options.get('part');
  if (part && !new RegExp(`\\b(tab|sub)\\("${part.replace('.', '\\.')}"`).test(manifest))
    throw new UsageError(`${part} is no part of ${feature}: its manifest declares none by that id`);
  const words = name.split('_');
  const title =
    args.options.get('title') ??
    [words[0]![0]!.toUpperCase() + words[0]!.slice(1), ...words.slice(1)].join(' ');
  const description = args.options.get('description') ?? 'Answer with the DSP it is about.';
  if (!/[.!?]$/.test(description.trim()))
    throw new UsageError('--description is a sentence or two, ending with a full stop');
  const declared = /switch: (?:optional|mandatory)\(\s*"([^"]+)",\s*"([^"]+)"/.exec(manifest);
  const values: Values = {
    feature,
    featureLabel: declared?.[2] ?? namesOf(feature).label,
    switchId: declared?.[1] ?? feature,
    name,
    pascal: namesOf(name).pascal,
    title: literal(title),
    description: literal(description.trim()),
    changes: args.flags.has('changes'),
    part: Boolean(part),
    partId: part ?? '',
    optional: /switch: optional\(/.test(manifest),
    ...installed(root, feature),
  };
  return { args, values, docs: wrapped(description.trim(), '/// ') };
}

/** Adds `line` to a crate's `[section]`, unless it lists that crate already. */
function depend(text: string, section: string, line: string) {
  const crate = line.split(' ')[0]!;
  const lines = text.split('\n');
  let header = lines.findIndex((each) => each.trim() === `[${section}]`);
  if (header < 0) {
    // The section goes before the first [[test]] or at the end.
    const at = lines.findIndex((each) => each.startsWith('[['));
    const insert = at < 0 ? lines.length : at;
    lines.splice(insert, 0, `[${section}]`, '', '');
    header = insert;
  }
  let end = lines.findIndex((each, index) => index > header && each.startsWith('['));
  if (end < 0) end = lines.length;
  if (lines.slice(header + 1, end).some((each) => each.startsWith(`${crate} `))) return text;
  // After the section's last entry, before the blank lines that end it.
  let last = end;
  while (last > header + 1 && !lines[last - 1]!.trim()) last--;
  lines.splice(last, 0, line);
  return lines.join('\n');
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
/** Has the manifest list its mcp/ module and name its tools. */
function wire(text: string, feature: string) {
  let wired = text;
  if (!/^mod mcp;$/m.test(wired)) wired = withModule(wired, 'mcp');
  if (!/^\s+tools: mcp::TOOLS,$/m.test(wired)) {
    const tail = new RegExp(`^(\\s+)\\.\\.feature\\("${feature}"\\)`, 'm').exec(wired);
    if (!tail)
      throw new Error(`features/${feature}/feature.rs has no ..feature("${feature}"); add tools`);
    wired = `${wired.slice(0, tail.index)}${tail[1]}tools: mcp::TOOLS,\n${wired.slice(tail.index)}`;
  }
  return wired;
}

export async function planTool(root: string, argv: string[]) {
  const { args, values, docs } = toolValues(root, argv);
  const { feature, name, pascal } = values as { feature: string; name: string; pascal: string };
  const dir = `features/${feature}`;
  const plan: Plan = emptyPlan();
  const tool = template('tool/tool.rs', values).replace(
    `/// ${values.description}\npub struct`,
    `${docs}\npub struct`,
  );
  plan.files.set(`${dir}/mcp/${name}.rs`, tool);
  plan.files.set(`${dir}/tests/backend/mcp/${name}.rs`, template('tool/test.rs', values));
  if (exists(root, `${dir}/mcp/mod.rs`)) {
    await change(plan, root, `${dir}/mcp/mod.rs`, (text) =>
      appendToList(withModule(text, name), TOOLS_LIST, `&${name}::${pascal},`, `${dir}/mcp/mod.rs`),
    );
  } else plan.files.set(`${dir}/mcp/mod.rs`, template('tool/mod.rs', values));
  await change(plan, root, `${dir}/feature.rs`, (text) => wire(text, feature));
  await change(plan, root, `${dir}/Cargo.toml`, (text) => {
    let crate = depend(text, 'dependencies', 'schemars = { workspace = true }');
    crate = depend(crate, 'dependencies', 'serde = { workspace = true }');
    crate = depend(crate, 'dev-dependencies', 'serde_json = { workspace = true }');
    return depend(
      crate,
      'dev-dependencies',
      'dispatch-core = { path = "../../core", features = ["testing"] }',
    );
  });
  formatRust(plan);
  plan.notes.push(
    `Make it do what it's for: its Input and Output in ${dir}/mcp/${name}.rs, and its test.`,
  );
  plan.notes.push(`Say what it does in ${dir}/README.md, beside what else the feature declares.`);
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
