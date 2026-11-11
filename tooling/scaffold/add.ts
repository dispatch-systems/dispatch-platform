import { featureDirectory, featureLocations } from '../shared/features.js';
import { renderFeature, settingsNote, showing } from './new-feature.js';
import { UsageError, names, parseArguments } from './common.js';
import { exists, features, collectors, files, pageOf, peopleSources, read } from './repository.js';
import { format, formatRust, type Plan } from './plan.js';
import { type Generator } from './scaffold.js';
import {
  closing,
  mergeImports,
  mergeRust,
  mergeToml,
  mergeTypeScript,
  routeKeys,
  rustfmt,
} from './source-editing.js';
import { installing } from './test-registry.js';

// `dispatchdev add <piece> --to <feature>`: a piece `dispatchdev new feature` writes, added to a
// feature that exists. The piece is rendered as the feature's own templates would write it, and
// what it adds to the feature's manifests is merged into them: imports, page declarations,
// constants, slot entries, fields and permissions. An entry joins a list the feature fills
// already; a field or function it declares already is left to its author, named in what to do
// next.

export const usage = `Usage: dispatchdev add <piece> --to <feature> [options]

  page                a page in a DSP's sidebar
  sidebar <pages>     a sidebar of its own pages, as entries,history: its item in a DSP's
                      sidebar opens their list in place of the app's
  tab-of <feature>    a tab of another feature's page
  settings            a section of a DSP's Settings, on a page of its own, that its backend
                      declares and reads: for those who manage it
  api                 api/ with one endpoint behind the view permission
  people <source>     who --table <table>, in the DSP's database or a collector's, names, for
                      Driver Match
  contract <name>     what it exposes to other features and agent tools, named for what it
                      offers: its contract crate in contract/, what fulfils it, and a test
  --shown             with contract, one a page shows: its name and the permission that shows
                      what it keeps, the feature's view, which offered(<name>) reads
  --icon <name>       its page's icon, from lucide-react
  --dry-run           print what it would write and change nothing
  --json              print what it made, or would make, as JSON
  --out <dir>         with --dry-run, write the files under <dir> instead
  --root <dir>        the repository to write into (default: this one)`;

/**
 * Each piece: whether it takes a value, its README line's label, and the permissions it uses,
 * which it adds where the feature lacks them.
 */
const PIECES: Record<string, { valued: boolean; readme: string; uses: string[] }> = {
  page: { valued: false, readme: 'Page', uses: ['view'] },
  sidebar: { valued: true, readme: 'Sidebar', uses: ['view'] },
  'tab-of': { valued: true, readme: 'Tab', uses: [] },
  settings: { valued: false, readme: 'Settings', uses: ['manage'] },
  api: { valued: false, readme: 'API', uses: ['view'] },
  people: { valued: true, readme: 'People', uses: [] },
  contract: { valued: true, readme: 'Exposed', uses: [] },
};
const FLAGS = ['dry-run', 'shown'];
const OPTIONS = ['to', 'table', 'icon', 'out', 'root'];

export async function planAdd(root: string, argv: string[]) {
  const args = parseArguments(argv, FLAGS, OPTIONS);
  const [piece, value, ...extra] = args.positional;
  const name = args.options.get('to');
  const known = piece ? PIECES[piece] : undefined;
  if (!known || extra.length || Boolean(value) !== known.valued)
    throw new UsageError(`Name one piece: ${Object.keys(PIECES).join(', ')}, with its value`);
  if (!name) throw new UsageError('Name the feature to add it to: --to <feature>');
  const shown = args.flags.has('shown');
  if (shown && piece !== 'contract')
    throw new UsageError('--shown is what a page shows of a contract: add contract <name> --shown');
  if (!features(root).includes(name))
    throw new UsageError(`--to names a feature, and ${name} is none`);
  const dir = featureDirectory(root, name);
  const custom = featureLocations(root).find((feature) => feature.name === name)?.owner;
  if (custom && piece === 'contract')
    throw new UsageError('Custom features cannot expose contracts');
  if (piece === 'api' && exists(root, `${dir}/api`))
    throw new UsageError(`${name} has api/ already: add its endpoints there`);
  if (piece === 'page' && pageOf(root, name))
    throw new UsageError(`${name} has a page already: add a tab of it with add tab-of ${name}`);
  const frontend = `${dir}/frontend/feature.ts`;
  if (piece === 'sidebar' && exists(root, frontend) && /\bsidebars:/.test(read(root, frontend)))
    throw new UsageError(
      `${name} has a sidebar of pages already: add a page to it, a route naming it its parent`,
    );
  // The page a feature had joins the sidebar it gains, listed there with the pages it adds,
  // unless it names a parent already, as a page of another feature's sidebar.
  const had = piece === 'sidebar' ? pageOf(root, name) : undefined;
  const under = (text: string, id: string) =>
    routeKeys(text, id)?.some((key) => key.name === 'parent');
  const joins = had && !under(read(root, frontend), had.id) ? had : undefined;
  if (piece === 'people' && !args.options.has('table'))
    throw new UsageError('people reads who a table names: add --table <table>');
  if (piece === 'contract' && exists(root, `${dir}/contract`))
    throw new UsageError(`${name} has contract/ already: add to the contract it has`);
  if (piece === 'settings' && exists(root, `${dir}/backend/settings.rs`))
    throw new UsageError(
      `${name} declares its settings in backend/settings.rs already: add a section or a field there`,
    );
  const manifest = read(root, `${dir}/feature.rs`);
  if (shown && !new RegExp(`\\bperm\\(\\s*"${name}\\.view"`).test(manifest))
    throw new UsageError(
      `A page shows ${value} to those holding ${name}.view, which ${name} lacks: add it, or show ${value} by hand`,
    );
  // A permission the piece uses that the feature retired can't come back.
  const retiredAt = manifest.indexOf('retired_identifiers: &[');
  const retired =
    retiredAt < 0 ? '' : manifest.slice(retiredAt, closing(manifest, retiredAt + 22) + 1);
  for (const use of known.uses) {
    const permission = `${name}.${use}`;
    if (retired.includes(`("${permission}"`))
      throw new UsageError(
        `${name} retired ${permission}, which the ${piece} would use: add it by hand`,
      );
  }
  if (piece === 'tab-of' && value === name)
    throw new UsageError(`A tab of ${name}'s own page is one of its own tabs: add it there`);
  // Its people's table, by the database the feature keeps it in.
  const tables =
    (
      JSON.parse(read(root, 'app/generated/features.json')) as Record<
        string,
        { tables: Record<string, string[]> }
      >
    )[name]?.tables ?? {};
  const table = args.options.get('table');
  const database = Object.entries(tables).find(([, list]) => table && list.includes(table))?.[0];
  if (
    piece === 'people' &&
    (!database || (database !== 'dsp' && !collectors(root).includes(database)))
  )
    throw new UsageError(
      database
        ? `${table} is in ${name}'s own database: connect its people by hand`
        : `--table names one of ${name}'s tables: ${Object.values(tables).flat().join(', ') || 'it has none'}`,
    );
  const label = /\b(?:optional|mandatory|custom)\(\s*"[^"]+",\s*"([^"]+)"/.exec(manifest)?.[1];
  const { pascal, slug } = names(name);
  // Its sidebar takes the feature's id, which its page of that id, with a parent, can't share.
  if (had && !joins && had.id === slug)
    throw new UsageError(
      `${name}'s page ${slug} has a parent already, and its sidebar would take its id: add it by hand, with an id of its own`,
    );
  // The piece as the feature's templates write it, for the feature as it is.
  const { plan: rendered } = await renderFeature(
    root,
    [
      name,
      ...(label ? ['--label', label] : []),
      ...(/\bswitch:\s*mandatory\(/.test(manifest) ? ['--mandatory'] : []),
      `--${piece}`,
      ...(value ? [value] : []),
      ...(piece === 'people' ? ['--tables', database!, '--table', table!] : []),
      ...(args.options.has('icon') ? ['--icon', args.options.get('icon')!] : []),
      ...(shown ? ['--shown'] : []),
    ],
    true,
  );
  const plan: Plan = { files: new Map(), changes: new Map(), notes: [] };
  const left: string[] = [];
  // A new feature's own storage and first migration, which a piece added to one never needs.
  const own = /\/(backend\/storage\.rs|tests\/backend\/storage\.rs|migrations\/.*)$/;
  const wanted = [...rendered.files].filter(([file]) => !own.test(file));
  const wrote = (file: string) => exists(root, file) || wanted.some(([each]) => each === file);
  const fresh = new Set(wanted.map(([file]) => file).filter((file) => !exists(root, file)));
  // What only the piece writes, as its screen or its people, is new: one there is the
  // feature's own, which the piece won't wire in its place.
  const merges =
    /\/(feature\.rs|frontend\/feature\.ts|Cargo\.toml|mod\.rs|routes\.txt|README\.md|frontend\/index\.ts|frontend\/platform-slots\.ts|[^/]+\.spec\.ts)$/;
  const there = wanted
    .map(([file]) => file)
    .filter((file) => !fresh.has(file) && !merges.test(file));
  if (there.length)
    throw new UsageError(`${name} has ${there.join(', ')} already: the ${piece} would write it`);
  for (const [file, content] of wanted) {
    if (!exists(root, file)) {
      plan.files.set(file, content);
      continue;
    }
    const current = read(root, file);
    let merged: string;
    if (file === `${dir}/feature.rs`)
      merged = mergeRust(current, content, left, known.uses, (module) =>
        fresh.has(`${dir}/${module}.rs`),
      );
    else if (file === `${dir}/frontend/feature.ts`) {
      merged = mergeTypeScript(current, content, pascal);
      // After its name and icon, as a page under a sidebar declares it.
      const keys = joins && routeKeys(merged, joins.id);
      const after =
        keys &&
        (keys.find((key) => key.name === 'icon') ?? keys.find((key) => key.name === 'label'));
      if (after)
        merged = `${merged.slice(0, after.end)}\nparent: '${slug}',${merged.slice(after.end)}`;
    } else if (file.endsWith('/Cargo.toml')) merged = mergeToml(current, content);
    else if (/\/mod\.rs$/.test(file))
      // Its modules whose files are there or written now.
      merged = content
        .split('\n')
        .filter((line) => {
          const module = /mod (\w+);/.exec(line)?.[1];
          const at = `${file.slice(0, file.lastIndexOf('/'))}/${module}`;
          const declared = new RegExp(`^(pub(\\([^)]*\\))? )?mod ${module};`, 'm');
          return module && !declared.test(current) && (wrote(`${at}.rs`) || exists(root, at));
        })
        .reduce((text, line) => {
          // After its last module, not one under an attribute as its tests are, or at its end
          // when it has none.
          const last = [...text.matchAll(/^(?:pub(?:\([^)]*\))? )?mod \w+;$/gm)]
            .filter((found) => !/#\[[^\n]*\]\n$/.test(text.slice(0, found.index)))
            .at(-1);
          if (!last) return `${text.trimEnd()}\n${line}\n`;
          const at = last.index! + last[0].length;
          return `${text.slice(0, at)}\n${line}${text.slice(at)}`;
        }, current);
    else if (file.endsWith('/routes.txt'))
      merged = content
        .split('\n')
        .filter((line) => /^[A-Z]+ \//.test(line) && !current.includes(line))
        .reduce((text, line) => `${text.trimEnd()}\n${line}\n`, current);
    else if (file.endsWith('/README.md'))
      // Its line, after the README's last line of what the feature declares.
      merged = content
        .split('\n')
        .filter((line) => line.startsWith(`- **${known.readme}:**`) && !current.includes(line))
        .reduce((text, line) => {
          const last = [...text.matchAll(/^- \*\*[^*]+\*\*.*(?:\n {2,}\S.*)*$/gm)].at(-1);
          if (!last) return `${text.trimEnd()}\n\n${line}\n`;
          const at = last.index! + last[0].length;
          return `${text.slice(0, at)}\n${line}${text.slice(at)}`;
        }, current);
    else if (/\/frontend\/index\.ts$/.test(file))
      // What its page's code exports.
      merged = content
        .split('\n')
        .filter((line) => line.startsWith('export ') && !current.includes(line))
        .reduce((text, line) => `${text.trimEnd()}\n${line}\n`, current);
    else if (file.endsWith('/platform-slots.ts')) continue;
    else if (file.endsWith('.spec.ts'))
      merged = `${mergeImports(current, content).trimEnd()}\n${content.slice(content.search(/\ntest\(/)).trimEnd()}\n`;
    else continue;
    if (merged !== current) plan.changes.set(file, await format(file, merged));
  }
  // A collector its people come from is one its tests install it with, as the registry wants
  // each feature's dependencies installed.
  const site = peopleSources(root).find((source) => source.id === value)?.site;
  if (piece === 'people' && site) installing(root, plan, dir, { collector: site });
  if (shown) await showing(root, plan, value!);
  // The test a contract or settings brings installs the feature with what it uses: the
  // collectors and features the feature map says it uses, what its other tests install, however
  // they name it, and what its manifest, as the piece leaves it, depends on.
  const tested = ({ contract: 'exposed', settings: 'settings' } as Record<string, string>)[piece!];
  if (tested) {
    const now = (file: string) => plan.changes.get(file) ?? read(root, file);
    const used = new Map<string, 'collector' | 'feature'>();
    const kindOf = (each: string) =>
      exists(root, `collectors/${each}/collector.rs`)
        ? 'collector'
        : each !== name && exists(root, `${featureDirectory(root, each)}/feature.rs`)
          ? 'feature'
          : undefined;
    const mapped = (
      JSON.parse(read(root, 'app/generated/features.json')) as Record<
        string,
        { collectors: string[]; depends_on: string[] }
      >
    )[name];
    for (const each of [...(mapped?.collectors ?? []), ...(mapped?.depends_on ?? [])]) {
      const kind = kindOf(each);
      if (kind) used.set(each, kind);
    }
    for (const file of files(root, dir, /\.rs$/))
      for (const [, collectors, features] of now(file).matchAll(
        /::install\(\s*&\[([^\]]*)\],\s*&\[([^\]]*)\]/g,
      ))
        for (const [, each] of `${collectors} ${features}`.matchAll(
          /&(?:dispatch_)?(\w+)::(?:COLLECTOR|FEATURE)\b/g,
        )) {
          const kind = kindOf(each!);
          if (kind) used.set(each!, kind);
        }
    const declared = /\bdepends_on:\s*&\[([^\]]*)\]/.exec(now(`${dir}/feature.rs`))?.[1] ?? '';
    for (const [, each] of declared.matchAll(/"([^"]+)"/g)) {
      const kind = kindOf(each!);
      if (kind) used.set(each!, kind);
    }
    for (const [each, kind] of used)
      installing(root, plan, dir, { [kind]: each }, `${dir}/tests/backend/${tested}.rs`);
  }
  if (piece === 'settings') plan.notes.push(settingsNote(name, slug));
  if (joins)
    plan.notes.push(
      `Its page ${joins.label} is under its sidebar now, with the pages added: a test opening it ` +
        "from the app's sidebar opens the sidebar first.",
    );
  // Its Rust as rustfmt leaves it: the files it writes together, and each it changes alone,
  // since that file's modules aren't beside it.
  formatRust(plan);
  for (const [file, content] of plan.changes)
    if (file.endsWith('.rs')) plan.changes.set(file, rustfmt(content));
  plan.notes.push(...left.map((note) => `By hand: ${note}.`));
  plan.notes.push('`dispatchdev generate` writes what follows from it.');
  return { plan, args };
}

export const generator: Generator = {
  kind: 'piece',
  about: 'a piece added to a feature that exists',
  usage,
  flags: FLAGS,
  options: OPTIONS,
  plan: planAdd,
  generates: true,
};
