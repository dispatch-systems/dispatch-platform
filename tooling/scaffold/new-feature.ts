import path from 'node:path';
import {
  UsageError,
  addAppFeature,
  addDependency,
  addFrontendFeature,
  addFrontendOwner,
  addWorkspaceMember,
  appBackend,
  appendToList,
  capabilitiesOf,
  change,
  collectionsOf,
  collectors,
  coreParts,
  drawsPageTabs,
  emptyPlan,
  exists,
  features,
  finish,
  format,
  formatRust,
  frontendList,
  holding,
  keeperOf,
  names as namesOf,
  nextAgentOrder,
  nextMigration,
  nextPermissionOrder,
  nextToggleGroupOrder,
  pageOf,
  parseArguments,
  repositoryRoot,
  routeInventory,
  run,
  snapshotsNote,
  template,
  typescriptExport,
  type Plan,
  type Values,
} from './scaffold.js';

// `npm run new:feature -- <name> [flags]`: a feature that works as written. It is a crate of its
// own, listed in app/, with a switch that starts off for every DSP and a view permission on the
// role sheet, and its first test. Each flag adds a piece with its starter code and its test.

export const usage = `Usage: npm run new:feature -- <name> [options]

  --always-on                       no switch: every DSP has it at once
  --api                             api/ with one endpoint behind the view permission
  --tables <database>               a first table: dsp, a collector's database, or the
                                    feature's own name with --keeps for a database of its own
  --keeps <collector>.<collection>  the keeper of a collector's collection
  --mcp                             mcp/ with one agent endpoint and its read toggle
  --page                            a page in a DSP's sidebar
  --tab-of <feature>                a tab of another feature's page
  --settings                        a tab on a DSP's Settings page
  --no-backend                      no backend/: a frontend-only feature, with --page,
                                    --tab-of or --settings
  --label <label>                   its name as people read it (default: from <name>)
  --dry-run                         print what it would write and change nothing
  --out <dir>                       with --dry-run, write the files under <dir> instead
  --root <dir>                      the repository to write into (default: this one)`;

const FLAGS = ['always-on', 'api', 'mcp', 'page', 'settings', 'no-backend', 'dry-run'];
const OPTIONS = ['tables', 'keeps', 'tab-of', 'label', 'out', 'root'];
/** Where the shell's frontend reads a DSP view's type from, relative to the root. */
const DSP_VIEW = 'core/accounts/api/index.js';

type Database =
  | { owner: 'core'; name: string }
  | { owner: 'collector'; name: string }
  | { owner: 'feature'; name: string };

/** Everything the templates are filled with, and which pieces the feature has. */
export function featureValues(root: string, argv: string[]) {
  const args = parseArguments(argv, FLAGS, OPTIONS);
  if (args.positional.length !== 1) throw new UsageError('Name one feature');
  const names = namesOf(args.positional[0]!, args.options.get('label'));
  const { name, slug, pascal, ident } = names;
  const taken = [...features(root), ...collectors(root), ...coreParts(root)];
  if (taken.includes(name) || exists(root, `features/${name}`))
    throw new UsageError(
      `${name} is taken: features, collectors and core's parts each own their name`,
    );

  const alwaysOn = args.flags.has('always-on');
  const api = args.flags.has('api');
  const mcp = args.flags.has('mcp');
  const page = args.flags.has('page');
  const settings = args.flags.has('settings');
  const host = args.options.get('tab-of');
  if (page && host)
    throw new UsageError('--page and --tab-of each give it its own screen: choose one');
  if (mcp && alwaysOn)
    throw new UsageError(
      'Agents read a feature through its switch: --mcp needs a feature with one',
    );
  const backend = !args.flags.has('no-backend');
  if (!backend) {
    if (!page && !host && !settings)
      throw new UsageError(
        '--no-backend leaves only a frontend: add --page, --tab-of or --settings',
      );
    const logic = ['api', 'tables', 'keeps', 'mcp'].filter(
      (piece) => args.flags.has(piece) || args.options.has(piece),
    );
    if (logic.length)
      throw new UsageError(
        `--no-backend leaves out the Rust logic that ${logic.map((piece) => `--${piece}`).join(', ')} needs`,
      );
  }

  const kept = args.options.get('keeps');
  let site = '';
  let collection = '';
  if (kept) {
    [site = '', collection = ''] = kept.split('.');
    if (!collectors(root).includes(site))
      throw new UsageError(`--keeps names a collector: one of ${collectors(root).join(', ')}`);
    if (!collectionsOf(root, site).includes(collection))
      throw new UsageError(
        `${site} collects ${collectionsOf(root, site).join(', ')}, not ${collection}`,
      );
    const keeper = keeperOf(root, site, collection);
    if (keeper)
      throw new UsageError(
        `${keeper} keeps ${site}.${collection} already: each collection has one keeper`,
      );
  }
  const keeps = Boolean(kept);

  const tables = args.options.get('tables');
  let database: Database | undefined;
  if (tables === 'dsp') database = { owner: 'core', name: 'dsp' };
  else if (tables && collectors(root).includes(tables))
    database = { owner: 'collector', name: tables };
  else if (tables === name && keeps) database = { owner: 'feature', name };
  else if (tables)
    throw new UsageError(
      `--tables takes dsp, a collector's database (${collectors(root).join(', ')}), or ${name} ` +
        `with --keeps for a database of its own beside the collector's`,
    );

  let hostPage = { id: '', label: '' };
  if (host) {
    if (!features(root).includes(host))
      throw new UsageError(`--tab-of names a feature, and ${host} is none`);
    const found = pageOf(root, host);
    if (!found) throw new UsageError(`${host} has no page to hold a tab`);
    // The pageTabs slot holds the tab; the host's page has to draw what it holds.
    if (!drawsPageTabs(root, host))
      throw new UsageError(
        `${host}'s page does not draw the tabs other features add to it yet: ` +
          `it needs pageTabs('${found.id}') among its own tabs first`,
      );
    hostPage = found;
  }

  const switched = !alwaysOn;
  // A switch needs a permission to gate its screens and routes; so does an endpoint or a keeper.
  const permission = switched || api || keeps;
  const order = nextPermissionOrder(root);
  const crateOf = (collector: string) => `dispatch-${collector.replace(/_/g, '-')}`;
  const identOf = (collector: string) => `dispatch_${collector}`;
  const used = [
    ...new Set([site, database?.owner === 'collector' ? database.name : ''].filter(Boolean)),
  ].sort();

  const own = database?.owner === 'feature';
  const migrationId = !database ? 0 : own ? 1 : nextMigration(root, database.name);
  const migrationName = own ? 'baseline' : name;
  const dependencies = [
    'dispatch-core = { path = "../../core" }',
    ...used.map(
      (collector) => `${crateOf(collector)} = { path = "../../collectors/${collector}" }`,
    ),
    ...(database ? ['rusqlite = { workspace = true }'] : []),
    ...(api || database ? ['serde = { workspace = true }'] : []),
    ...(mcp ? ['serde_json = { workspace = true }'] : []),
    ...(api ? ['ts-rs = { workspace = true, optional = true }'] : []),
  ];
  const devDependencies = [
    ...(database || keeps
      ? ['dispatch-core = { path = "../../core", features = ["testing"] }']
      : []),
    ...(keeps && !mcp ? ['serde_json = { workspace = true }'] : []),
  ];
  const databaseFile = !database
    ? ''
    : database.owner === 'core'
      ? 'dispatch.sqlite'
      : `${database.name}/${database.name}.sqlite`;
  const open = !database
    ? ''
    : database.owner === 'core'
      ? 'self.dsp(dsp)?'
      : database.owner === 'collector'
        ? `self.collector(dsp, ${identOf(database.name)}::PROVIDER)?`
        : `self.added_storage(dsp, ${identOf(site)}::PROVIDER, &STORAGE)?`;
  const kind = !database
    ? ''
    : database.owner === 'core'
      ? 'Kind::DSP'
      : database.owner === 'collector'
        ? `${identOf(database.name)}::DATABASE`
        : 'backend::storage::DATABASE';
  const manifestImports = [
    'Feature',
    ...(switched ? ['Switch'] : []),
    'feature',
    ...(permission ? ['perm'] : []),
  ];
  const databaseLabel = !database
    ? ''
    : database.owner === 'core'
      ? "each DSP's own database"
      : database.owner === 'collector'
        ? `each DSP's \`${database.name}\` database`
        : `a database of its own beside ${namesOf(site).label}'s`;
  // Parameters a starter body does not read yet are named so the compiler does not warn.
  const parameters = (read: boolean, list: [string, string][]) =>
    list.map(([parameter, type]) => `${read ? parameter : `_${parameter}`}: ${type}`).join(', ');

  const screens = page || Boolean(host) || settings;
  const values: Values = {
    ...names,
    backend,
    switch: switched,
    permission,
    viewOrder: order,
    collectOrder: order + 1,
    bare: !switched && !permission && !database && !keeps && !mcp,
    manifestImports: `{${manifestImports.join(', ')}}`,
    requires: keeps && capabilitiesOf(root, site).includes(collection) ? `"${collection}"` : '',
    dependencies: dependencies.join('\n'),
    devDependencies: devDependencies.join('\n'),
    api,
    apiPath: `/api/dsp/${slug}`,
    summaryParameters: `${database ? 'db' : '_'}: &Store, c: &Member, _: &Input`,
    tables: Boolean(database),
    database: database?.name ?? '',
    databaseLabel,
    ownDatabase: own,
    databaseFile,
    dbImports: !database
      ? ''
      : `{${database.owner === 'core' ? 'Kind, ' : ''}Migration, Migrations, migrations::Apply::Sql}`,
    kind,
    open,
    table: `${name}_items`,
    migrationId,
    migrationName,
    migrationFile: `${String(migrationId).padStart(4, '0')}_${migrationName}.sql`,
    storageExports: `{${pascal}Item, ${pascal}Store}`,
    keeps,
    site,
    siteIdent: site ? identOf(site) : '',
    siteLabel: site ? namesOf(site).label : '',
    collection,
    fixtureRequest: `{"collection": "${collection}"}`,
    publishParameters: `${parameters(Boolean(database), [
      ['store', '&Store'],
      ['dsp', '&str'],
      ['job', '&str'],
    ])}, _collected: Collected`,
    testCollectors: used.map((collector) => `&${identOf(collector)}::COLLECTOR`).join(', '),
    mcp,
    agentOrder: mcp ? nextAgentOrder(root) : 0,
    toggleGroupOrder: mcp ? nextToggleGroupOrder(root) : 0,
    // A switch shows on the DSPs page with its icon, which the frontend's platform slots give.
    frontend: screens || switched,
    screens,
    platformSlots: switched,
    canImport: permission && (page || settings),
    labelPattern: names.label.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&'),
    page,
    tab: Boolean(host),
    settings,
    prefetch: page && api,
    pageKey: /^[A-Za-z_$][\w$]*$/.test(slug) ? slug : `'${slug}'`,
    host: host ?? '',
    hostPage: hostPage.id,
    hostLabel: hostPage.label,
    dspView: DSP_VIEW,
  };
  return { args, names, values, ident };
}

/** What is written where, given the feature's pieces: [piece, template, path in the feature]. */
function pieces(values: Values): [boolean, string, string][] {
  const { slug, pascal, database, migrationFile } = values;
  return [
    [true, 'Cargo.toml', 'Cargo.toml'],
    [true, 'feature.rs', 'feature.rs'],
    [true, 'README.md', 'README.md'],
    [values.backend as boolean, 'backend/mod.rs', 'backend/mod.rs'],
    [values.backend as boolean, 'tests/backend/feature.rs', 'tests/backend/feature.rs'],
    [values.api as boolean, 'api/mod.rs', 'api/mod.rs'],
    [values.api as boolean, 'api/routes.rs', 'api/routes.rs'],
    [values.api as boolean, 'api/types.rs', 'api/types.rs'],
    [values.api as boolean, 'api/client.ts', 'api/client.ts'],
    [values.api as boolean, 'tests/api/api.test.ts', `tests/api/${slug}.test.ts`],
    [values.tables as boolean, 'migrations/first.sql', `migrations/${database}/${migrationFile}`],
    [values.tables as boolean, 'backend/storage.rs', 'backend/storage.rs'],
    [values.tables as boolean, 'tests/backend/storage.rs', 'tests/backend/storage.rs'],
    [values.keeps as boolean, 'backend/keeper.rs', 'backend/keeper.rs'],
    [values.keeps as boolean, 'tests/backend/keeper.rs', 'tests/backend/keeper.rs'],
    [values.mcp as boolean, 'mcp/mod.rs', 'mcp/mod.rs'],
    [values.mcp as boolean, 'mcp/catalog.rs', 'mcp/catalog.rs'],
    [values.mcp as boolean, 'mcp/views.rs', 'mcp/views.rs'],
    [values.mcp as boolean, 'tests/mcp/questions.ts', 'tests/mcp/questions.ts'],
    [values.frontend as boolean, 'frontend/feature.ts', 'frontend/feature.ts'],
    [values.page as boolean, 'frontend/index.ts', 'frontend/index.ts'],
    [values.page as boolean, 'frontend/Page.tsx', `frontend/${pascal}Page.tsx`],
    // frontend/tabs/ holds the tabs of its own page, which its manifest declares.
    [values.tab as boolean, 'frontend/Tab.tsx', `frontend/${pascal}Tab.tsx`],
    [
      values.settings as boolean,
      'frontend/Settings.tsx',
      `frontend/settings/${pascal}Settings.tsx`,
    ],
    [values.platformSlots as boolean, 'frontend/platform-slots.ts', 'frontend/platform-slots.ts'],
    [values.frontend as boolean, 'tests/browser/browser.spec.ts', `tests/browser/${slug}.spec.ts`],
  ];
}

export async function planFeature(root: string, argv: string[]) {
  const { args, names, values, ident } = featureValues(root, argv);
  const { name, slug, camel, pascal, crate } = names;
  const dir = `features/${name}`;
  const plan: Plan = emptyPlan();
  for (const [wanted, source, target] of pieces(values)) {
    if (!wanted) continue;
    const file = `${dir}/${target}`;
    const toRoot = '../'.repeat(path.posix.dirname(file).split('/').length);
    const content = template(`feature/${source}`, { ...values, toRoot });
    plan.files.set(file, await format(file, content));
  }
  formatRust(plan);

  // List it in app/: the workspace, the app's dependencies and its Cargo feature, the
  // registry, the route table's inventory and the frontend's order and list. Its manifest
  // brings its routes.
  const edit = (file: string, apply: (text: string) => string) => change(plan, root, file, apply);
  await edit('Cargo.toml', (text) => addWorkspaceMember(text, dir));
  await edit('app/backend/Cargo.toml', (text) => {
    const app = 'app/backend/Cargo.toml';
    const listed = addAppFeature(
      addDependency(text, crate, `../../${dir}`, app, { optional: true }),
      name,
      crate,
      app,
    );
    // The app's export test writes its API types' TypeScript, as it does every feature's.
    return values.api
      ? addDependency(listed, crate, `../../${dir}`, app, {
          section: 'dev-dependencies',
          features: ['ts'],
        })
      : listed;
  });
  // The registry has it while the app's Cargo feature does.
  const registry = holding(root, appBackend, /pub static REGISTRY\b/);
  const featureList = /pub static REGISTRY\b[\s\S]*?features:\s*&\[/;
  await edit(registry, (text) =>
    [`#[cfg(feature = "${name}")]`, `&${ident}::FEATURE,`].reduce(
      (listed, line) => appendToList(listed, featureList, line, registry),
      text,
    ),
  );
  if (values.frontend) await edit(registry, (text) => addFrontendOwner(text, dir, registry));
  // The app's export test writes its API type to its api/generated/ while the build has it.
  if (values.api)
    await edit(typescriptExport, (text) => {
      const anchor = '    bindings.insert(ACCESS_CATALOG.into(), access_catalog());\n';
      if (!text.includes(anchor))
        throw new Error(
          `${typescriptExport}: cannot find its last binding; list ${name}'s by hand`,
        );
      return text.replace(
        anchor,
        `    #[cfg(feature = "${name}")]\n` +
          `    bindings.extend(exported!(&cfg, ${ident}::${pascal}Summary));\n${anchor}`,
      );
    });
  const rows = [
    ...(values.api ? [`("GET", "${values.apiPath}", Dsp("${name}.view"), Read, false),`] : []),
    ...(values.mcp ? [`("GET", "/api/v1/${slug}", Agent("read"), Read, false),`] : []),
  ];
  for (const row of rows)
    await edit(routeInventory, (text) =>
      appendToList(text, /const INVENTORY: &\[Row\] = &\[/, row, routeInventory),
    );
  if (values.frontend)
    await edit(frontendList, (text) =>
      addFrontendFeature(text, camel, `../../${dir}/frontend/feature.js`),
    );

  const declared = [values.switch && 'switch', values.permission && 'permissions'].filter(Boolean);
  const generated = [
    'the feature map',
    ...(declared.length ? [`the catalog with its ${declared.join(' and ')}`] : []),
    ...(values.api ? [`${dir}/api/generated/`] : []),
  ];
  plan.notes.push(`\`npm run contracts:generate\` writes ${generated.join(', and ')}.`);
  plan.notes.push(snapshotsNote);
  if (values.tables && !values.ownDatabase)
    plan.notes.push(
      `It takes migration ${values.migrationId} of the ${values.database} database, after every ` +
        'one declared or recorded in the migrations history.',
    );
  plan.notes.push(`\`npm run test:feature ${name}\` runs its tests of every kind.`);
  return { plan, args };
}

export async function main(argv: string[]) {
  const options = parseArguments(argv, FLAGS, OPTIONS);
  const root = path.resolve(options.options.get('root') ?? repositoryRoot);
  const { plan, args } = await planFeature(root, argv);
  const dryRun = args.flags.has('dry-run');
  if (args.options.has('out') && !dryRun) throw new UsageError('--out goes with --dry-run');
  finish(plan, root, { dryRun, out: args.options.get('out') });
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(import.meta.filename))
  await run(usage, main);
