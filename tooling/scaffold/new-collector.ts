import path from 'node:path';
import {
  UsageError,
  addDependency,
  addFrontendFeature,
  addFrontendOwner,
  addWorkspaceMember,
  appBackend,
  appendToList,
  collectors,
  coreParts,
  current,
  emptyPlan,
  exists,
  forwardFeature,
  features,
  finish,
  format,
  formatRust,
  frontendList,
  holding,
  names as namesOf,
  offersSlot,
  parseArguments,
  repositoryRoot,
  run,
  template,
  type Plan,
  type Values,
} from './scaffold.js';

// `npm run new:collector -- <site> [flags]`: a collector for one outside site, as a crate of its
// own listed in app/: its connection, one collection with its fixture, its connection card and a
// native test that signs in to a staged copy of the site.

export const usage = `Usage: npm run new:collector -- <site> [options]

  --collection <name>   its first collection (default: records)
  --host <host>         the site's host, the only one its browser may open
                        (default: <site>.example.com)
  --label <label>       its name as people read it (default: from <site>)
  --dry-run             print what it would write and change nothing
  --out <dir>           with --dry-run, write the files under <dir> instead
  --root <dir>          the repository to write into (default: this one)`;

const FLAGS = ['dry-run'];
const OPTIONS = ['collection', 'host', 'label', 'out', 'root'];
const testPlan = 'tooling/ci/test-plan.json';
const workflow = '.github/workflows/checks.yml';

export function collectorValues(root: string, argv: string[]) {
  const args = parseArguments(argv, FLAGS, OPTIONS);
  if (args.positional.length !== 1) throw new UsageError('Name one site');
  const names = namesOf(args.positional[0]!, args.options.get('label'));
  const { name, slug } = names;
  const taken = [...features(root), ...collectors(root), ...coreParts(root)];
  if (taken.includes(name) || exists(root, `collectors/${name}`))
    throw new UsageError(
      `${name} is taken: features, collectors and core's parts each own their name`,
    );
  const collection = namesOf(args.options.get('collection') ?? 'records');
  const host = args.options.get('host') ?? `${slug}.example.com`;
  if (!/^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$/.test(host))
    throw new UsageError(`${host} is not a host name`);
  const values: Values = {
    ...names,
    collection: collection.name,
    collectionTitle: collection.label,
    collectionLabel: collection.label.toLowerCase(),
    host,
  };
  return { args, names, values };
}

function pieces(values: Values): [string, string][] {
  const { name, slug, collection } = values;
  return [
    ['Cargo.toml', 'Cargo.toml'],
    ['collector.rs', 'collector.rs'],
    ['README.md', 'README.md'],
    ['connection/mod.rs', 'connection/mod.rs'],
    ['collections/mod.rs', 'collections/mod.rs'],
    ['collections/collection.rs', `collections/${collection}/mod.rs`],
    ['fixtures/mod.rs', 'fixtures/mod.rs'],
    ['fixtures/collection.rs', `fixtures/${collection}.rs`],
    ['scripts/auth.js', 'scripts/auth.js'],
    ['scripts/collection.js', `scripts/${collection}.js`],
    ['migrations/baseline.sql', `migrations/${name}/0001_baseline.sql`],
    ['frontend/feature.ts', 'frontend/feature.ts'],
    ['tests/backend/collection.rs', `tests/backend/${collection}.rs`],
    ['tests/native/worker.test.ts', `tests/native/${slug}-worker.test.ts`],
  ];
}

/** Adds a native shard of `files`, so its suite runs with a real browser in CI. */
function addNativeShard(text: string, shard: string, files: string[]) {
  const native = /"native":\s*\{/.exec(text);
  if (!native) throw new Error(`${testPlan} has no native shards`);
  const close = text.indexOf('\n  }', native.index);
  const entry = `,\n    "${shard}": [${files.map((file) => `"${file}"`).join(', ')}]`;
  return `${text.slice(0, close)}${entry}${text.slice(close)}`;
}
/** Adds the shard to the collectors job's matrix. */
function addWorkflowShard(text: string, shard: string) {
  const matrix = /^(\s*shard: \[)([^\]\n]*\bcapacity\b[^\]\n]*)(\])/m.exec(text);
  if (!matrix)
    throw new Error(`${workflow}: cannot find the collectors job's shards; add ${shard} by hand`);
  const shards = matrix[2]!.split(',').map((item) => item.trim());
  const at = shards.indexOf('capacity');
  shards.splice(at, 0, shard);
  return text.replace(matrix[0], `${matrix[1]}${shards.join(', ')}${matrix[3]}`);
}

export async function planCollector(root: string, argv: string[]) {
  const { args, names, values } = collectorValues(root, argv);
  const { name, slug, camel, crate, ident } = names;
  const dir = `collectors/${name}`;
  const plan: Plan = emptyPlan();
  for (const [source, target] of pieces(values)) {
    const file = `${dir}/${target}`;
    const toRoot = '../'.repeat(path.posix.dirname(file).split('/').length);
    const content = template(`collector/${source}`, { ...values, toRoot });
    plan.files.set(file, await format(file, content));
  }
  formatRust(plan);

  const change = async (file: string, edit: (text: string) => string) =>
    plan.changes.set(file, await format(file, edit(current(plan, root, file))));
  await change('Cargo.toml', (text) => addWorkspaceMember(text, dir));
  await change('app/backend/Cargo.toml', (text) =>
    forwardFeature(
      addDependency(text, crate, `../../${dir}`, 'app/backend/Cargo.toml'),
      'operator-probes',
      crate,
      'app/backend/Cargo.toml',
    ),
  );
  const registry = holding(root, appBackend, /pub static REGISTRY\b/);
  await change(registry, (text) =>
    appendToList(
      text,
      /pub static REGISTRY\b[\s\S]*?collectors:\s*&\[/,
      `&${ident}::COLLECTOR,`,
      registry,
    ),
  );
  await change(registry, (text) => addFrontendOwner(text, dir, registry));
  await change(frontendList, (text) =>
    addFrontendFeature(text, camel, `../../${dir}/frontend/feature.js`),
  );
  const native = `${dir}/tests/native/${slug}-worker.test.ts`;
  await change(testPlan, (text) => addNativeShard(text, slug, [native]));
  if (exists(root, workflow)) await change(workflow, (text) => addWorkflowShard(text, slug));

  const { collection } = values;
  plan.notes.push(
    `Each collection has exactly one keeper, and the registry refuses ${name}.${collection} ` +
      `without one: \`npm run new:feature -- <name> --keeps ${name}.${collection}\`.`,
  );
  plan.notes.push(
    `Jobs and schedules store their kind under a CHECK: widen it for ${name}.${collection}.collect ` +
      `and the ${collection} schedule with a migration, as core/collection/migrations/jobs/0004_dvic_kind.sql did.`,
  );
  if (!offersSlot(root, 'connectionCards'))
    plan.notes.push(
      "core/shell's FrontendFeature has no connectionCards slot yet: add it so the card shows.",
    );
  plan.notes.push('`npm run contracts:generate` writes the catalog with its connection switch.');
  if (exists(root, 'app/tests/backend/catalog.rs'))
    plan.notes.push(
      'app/tests/backend/catalog.rs holds the catalog as it stood: add its connection.',
    );
  plan.notes.push(
    `\`npm run test:collector ${name}\` runs its tests, the native shard \`${slug}\` included.`,
  );
  return { plan, args };
}

export async function main(argv: string[]) {
  const options = parseArguments(argv, FLAGS, OPTIONS);
  const root = path.resolve(options.options.get('root') ?? repositoryRoot);
  const { plan, args } = await planCollector(root, argv);
  const dryRun = args.flags.has('dry-run');
  if (args.options.has('out') && !dryRun) throw new UsageError('--out goes with --dry-run');
  finish(plan, root, { dryRun, out: args.options.get('out') });
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(import.meta.filename))
  await run(usage, main);
