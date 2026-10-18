import path from 'node:path';
import {
  UsageError,
  addWorkspaceMember,
  capabilitiesOf,
  change,
  collectionsOf,
  collectors,
  coreParts,
  emptyPlan,
  exists,
  features,
  format,
  formatRust,
  isIcon,
  keeperOf,
  names as namesOf,
  nextPeopleOrder,
  nextPermissionOrder,
  nextSourceOrder,
  ownPermissionOrder,
  nextPlace,
  pageOf,
  parseArguments,
  peopleSources,
  read,
  run,
  snapshotsNote,
  template,
  type Generator,
  type Plan,
  type Values,
} from './scaffold.js';
import {
  appManifest,
  crateOf,
  crates,
  featureList,
  needed,
  tools,
  wiredList,
  wiredManifest,
} from './wire.js';

// `dispatchdev new feature <name> [options]`: a feature that works as written. It is a crate of its
// own, listed in app/, optional, with a switch that starts off for every DSP, which a browser test finds on
// the DSPs page, and a view permission on the role sheet. Each flag adds a piece with its starter
// code and its test.

export const usage = `Usage: dispatchdev new feature <name> [options]

  --mandatory                       every DSP has it, with no switch to turn it off
  --api                             api/ with one endpoint behind the view permission
  --tables <database>               a first table: dsp, a collector's database, or the
                                    feature's own name with --keeps for a database of its own
  --keeps <collector>.<collection>  the keeper of a collector's collection
  --people <source>                 who its table names, connected to Driver Match by their
                                    IDs of a collector's source, as amazon, or by IDs of its
                                    own with own; needs --tables
  --page                            a page in a DSP's sidebar
  --tabs <ids>                      a page with tabs of its own, as overview,history, among
                                    which it draws those other features add to it
  --tab-of <feature>                a tab of another feature's page
  --settings                        a tab on a DSP's Settings page, for those who manage it
  --connection                      a card on a DSP's Connections tab, for one of the DSP's
                                    own accounts
  --contract <name>                 what it exposes to other features and agent tools, named
                                    for what it offers, as short_inspections: its contract
                                    crate in contract/, what fulfils it, and their test
  --shown                           with --contract, one a page shows: its name and the
                                    permission that shows what it keeps, which offered(<name>)
                                    reads
  --no-backend                      no backend/: a frontend-only feature, with --page,
                                    --tab-of or --settings
  --label <label>                   its name as people read it (default: from <name>)
  --icon <name>                     its icon on its page, its switch and its people: a
                                    lucide-react icon, as Truck (default: LayoutGrid)
  --preset <name>                   a common shape, with the options above it stands for:
                                    page-with-tabs  --tabs overview,history --api
                                    settings        --settings --api
                                    keeper          --keeps ... --tables <name> --page --api
                                    connection      --connection --api
                                    people-source   --tables dsp --people own --page --api
  --dry-run                         print what it would write and change nothing
  --out <dir>                       with --dry-run, write the files under <dir> instead
  --root <dir>                      the repository to write into (default: this one)`;

/** The presets: each a common shape, as the options it stands for. */
const PRESETS: Record<string, (name: string) => string[]> = {
  'page-with-tabs': () => ['--tabs', 'overview,history', '--api'],
  settings: () => ['--settings', '--api'],
  keeper: (name) => ['--tables', name, '--page', '--api'],
  connection: () => ['--connection', '--api'],
  'people-source': () => ['--tables', 'dsp', '--people', 'own', '--page', '--api'],
};
/** `argv` with its preset, if it names one, as the options it stands for. */
export function withPreset(argv: string[]) {
  const at = argv.findIndex((arg) => arg === '--preset' || arg.startsWith('--preset='));
  if (at < 0) return argv;
  const inline = argv[at]!.includes('=') ? argv[at]!.split('=')[1] : undefined;
  const preset = inline ?? argv[at + 1];
  const rest = argv.filter((_, index) => index !== at && (inline || index !== at + 1));
  const expand = preset ? PRESETS[preset] : undefined;
  if (!expand) throw new UsageError(`--preset names one of ${Object.keys(PRESETS).join(', ')}`);
  const name = rest.find((arg) => !arg.startsWith('--')) ?? '';
  if (preset === 'keeper' && !rest.some((arg) => arg.startsWith('--keeps')))
    throw new UsageError(
      '--preset keeper keeps a collection: add --keeps <collector>.<collection>',
    );
  // The options given after it win over its own.
  return [...expand(name), ...rest];
}

const FLAGS = [
  'mandatory',
  'api',
  'page',
  'settings',
  'connection',
  'no-backend',
  'shown',
  'dry-run',
];
const OPTIONS = [
  'tables',
  'keeps',
  'people',
  'tab-of',
  'tabs',
  'contract',
  'label',
  'icon',
  'preset',
  'out',
  'root',
];
/** Where the shell's frontend reads a DSP view's type from, relative to the root. */
const DSP_VIEW = 'core/accounts/api/index.js';

type Database =
  | { owner: 'core'; name: string }
  | { owner: 'collector'; name: string }
  | { owner: 'feature'; name: string };

/**
 * Everything the templates are filled with, and which pieces the feature has. `adding` fills
 * them for pieces added to a feature that exists (`dispatchdev add`): its permissions follow its
 * own, and `--table` names the table its people are read from.
 */
export function featureValues(root: string, argv: string[], adding = false) {
  const args = parseArguments(withPreset(argv), FLAGS, adding ? [...OPTIONS, 'table'] : OPTIONS);
  if (args.positional.length !== 1) throw new UsageError('Name one feature');
  const names = namesOf(args.positional[0]!, args.options.get('label'));
  const { name, slug, pascal } = names;
  const taken = [...features(root), ...collectors(root), ...coreParts(root)];
  if (!adding && (taken.includes(name) || exists(root, `features/${name}`)))
    throw new UsageError(
      `${name} is taken: features, collectors and core's parts each own their name`,
    );

  const icon = args.options.get('icon') ?? 'LayoutGrid';
  if (args.options.has('icon') && !isIcon(icon))
    throw new UsageError(
      `--icon names a lucide-react icon, as Truck or ClipboardList: ${icon} is none`,
    );

  const mandatory = args.flags.has('mandatory');
  const api = args.flags.has('api');
  const ownTabs = args.options.get('tabs')?.split(',');
  if (ownTabs && !ownTabs.every((id) => /^[a-z][a-z0-9-]*$/.test(id)))
    throw new UsageError(
      '--tabs names each tab as lowercase letters, digits and hyphens: overview,history',
    );
  const page = args.flags.has('page') || Boolean(ownTabs);
  const connection = args.flags.has('connection');
  const settings = args.flags.has('settings');
  const host = args.options.get('tab-of');
  if (page && host)
    throw new UsageError('--page and --tab-of each give it its own screen: choose one');
  if (args.flags.has('no-backend')) {
    if (!page && !host && !settings)
      throw new UsageError(
        '--no-backend leaves only a frontend: add --page, --tab-of or --settings',
      );
    const logic = ['api', 'tables', 'keeps', 'people', 'contract'].filter(
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

  // Who its table names, by the IDs of a source a collector declares.
  const sources = peopleSources(root);
  const named = args.options.get('people');
  // `own`: IDs of a source of its own, which it declares beside its people.
  const ownSource = named === 'own';
  const source = ownSource
    ? { id: name, site: '' }
    : named === undefined
      ? undefined
      : sources.find((each) => each.id === named);
  if (named !== undefined && !source)
    throw new UsageError(
      `--people names a source of IDs: one of ${sources.map((each) => each.id).join(', ')}, or own`,
    );
  if (source && !database)
    throw new UsageError('--people reads who it names from its table: add --tables');

  let hostPage = { id: '', label: '' };
  if (host) {
    if (!features(root).includes(host))
      throw new UsageError(`--tab-of names a feature, and ${host} is none`);
    const found = pageOf(root, host);
    if (!found) throw new UsageError(`${host} has no page to hold a tab`);
    // Every page draws the tabs other features add to it, but Settings' tabs are a slot of
    // their own.
    if (found.hostsSettings)
      throw new UsageError(`${host}'s page draws Settings' tabs: use --settings for one`);
    hostPage = found;
  }

  // What it exposes, by its name, as no other feature exposes anything.
  const contract = args.options.get('contract');
  if (contract !== undefined) {
    if (!/^[a-z][a-z0-9]*(_[a-z0-9]+)*$/.test(contract))
      throw new UsageError(
        `--contract names what it offers in lowercase words, as short_inspections: ${contract} isn't`,
      );
    const map = exists(root, 'app/generated/features.json')
      ? (JSON.parse(read(root, 'app/generated/features.json')) as Record<
          string,
          { slots: { exposes?: { name: string }[] } }
        >)
      : {};
    // As generated, and as each manifest says now, which generate has yet to read.
    const taken =
      Object.entries(map).find(([, feature]) =>
        (feature.slots.exposes ?? []).some((exposed) => exposed.name === contract),
      )?.[0] ??
      features(root).find((owner) =>
        [
          ...read(root, `features/${owner}/feature.rs`).matchAll(
            /\bexpose!\(\s*[^,]+,\s*[^,]+,\s*"([^"]+)"/g,
          ),
        ].some((found) => found[1] === contract),
      );
    if (taken) throw new UsageError(`${taken} exposes ${contract} already`);
  }
  const shown = args.flags.has('shown');
  if (shown && contract === undefined)
    throw new UsageError('--shown is what a page shows of what it exposes: add --contract <name>');
  const contractNames = contract ? namesOf(contract) : undefined;
  const contractCrate = `dispatch-${name.replace(/_/g, '-')}-contract`;

  const switched = !mandatory;
  // A switch needs a permission to gate its screens and routes; so does an endpoint, a keeper
  // or a Settings tab, which those who manage it see.
  const permission = switched || api || keeps || settings;
  if (shown && !permission && !adding)
    throw new UsageError(
      '--shown needs the permission that shows what it keeps: add --api, or leave out --mandatory',
    );
  const order = adding ? ownPermissionOrder(root, name) : nextPermissionOrder(root);
  const crateOf = (collector: string) => `dispatch-${collector.replace(/_/g, '-')}`;
  const identOf = (collector: string) => `dispatch_${collector}`;
  const used = [
    ...new Set(
      [site, database?.owner === 'collector' ? database.name : '', source?.site ?? ''].filter(
        Boolean,
      ),
    ),
  ].sort();

  const own = database?.owner === 'feature';
  // Its own database's list, or what it numbers itself in one other owners add to: either
  // way its first.
  const migrationId = database ? 1 : 0;
  const migrationName = own ? 'baseline' : name;
  const dependencies = [
    'dispatch-core = { path = "../../core" }',
    ...used.map(
      (collector) => `${crateOf(collector)} = { path = "../../collectors/${collector}" }`,
    ),
    ...(database ? ['rusqlite = { workspace = true }'] : []),
    ...(api || database ? ['serde = { workspace = true }'] : []),
    ...(api ? ['ts-rs = { workspace = true, optional = true }'] : []),
    ...(contract ? [`${contractCrate} = { path = "contract" }`] : []),
  ];
  const devDependencies = [
    ...(database || keeps || contract
      ? ['dispatch-core = { path = "../../core", features = ["testing"] }']
      : []),
    ...(keeps ? ['serde_json = { workspace = true }'] : []),
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
    'feature',
    switched ? 'optional' : 'mandatory',
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

  const screens = page || Boolean(host) || settings || connection;
  // A switch shows on the DSPs page with its icon, which the frontend's platform slots give.
  const frontend = screens || switched;
  // backend/ holds its storage and keeper. A feature with neither, no API and no frontend, as
  // --mandatory alone, keeps an empty one and its manifest's test: the anatomy rule wants a
  // backend, an API or a frontend.
  const manifestOnly = !api && !frontend && !database && !keeps && !contract;
  const values: Values = {
    ...names,
    backend: Boolean(database) || keeps || manifestOnly || Boolean(contract),
    manifestTest: manifestOnly,
    switch: switched,
    permission,
    viewOrder: order,
    collectOrder: order + 1,
    manageOrder: order + (keeps ? 2 : 1),
    // What it uses beyond the collector whose collection it keeps: its database's, its people's.
    dependsOn: used
      .filter((collector) => collector !== site)
      .map((collector) => `"${collector}"`)
      .join(', '),
    place: nextPlace(root),
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
      : `{${database.owner === 'core' ? 'Kind, ' : ''}Migration, ${own ? 'Migrations' : 'OwnMigrations'}, migrations::Apply::Sql}`,
    kind,
    open,
    table: args.options.get('table') ?? `${name}_items`,
    migrationId,
    migrationName,
    migrationFile: `${String(migrationId).padStart(4, '0')}_${migrationName}.sql`,
    storageExports: `{${pascal}Item, ${pascal}Store}`,
    keeps,
    people: Boolean(source),
    peopleOrder: nextPeopleOrder(root),
    icon,
    peopleIcon: args.options.has('icon') ? icon : 'Users',
    peopleSource: ownSource ? names.label : source ? namesOf(source.id).label : '',
    ownSource,
    sourceOrder: nextSourceOrder(root),
    ownTabs: Boolean(ownTabs),
    tabDefault: ownTabs?.[0] ?? '',
    tabItems: (ownTabs ?? [])
      .map((id) => `['${id}', '${namesOf(id.replace(/-/g, '_')).label}']`)
      .join(', '),
    tabBlocks: (ownTabs ?? [])
      .map(
        (id) =>
          `      {tab === '${id}' && (\n        <>{/* TODO(scaffold): what the ${namesOf(id.replace(/-/g, '_')).label} tab shows. */}</>\n      )}`,
      )
      .join('\n'),
    connection,
    peopleIdent: source ? identOf(source.site) : '',
    readPeople: open.replace(/^self\./, 'store.'),
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
    frontend,
    screens,
    platformSlots: switched,
    canImport: permission && (page || settings),
    labelPattern: names.label.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&'),
    page,
    tab: Boolean(host),
    settings,
    prefetch: page && api,
    summaryUrl: api && (page || connection),
    pageKey: /^[A-Za-z_$][\w$]*$/.test(slug) ? slug : `'${slug}'`,
    host: host ?? '',
    hostPage: hostPage.id,
    hostLabel: hostPage.label,
    dspView: DSP_VIEW,
    contract: contract ?? '',
    contractPascal: contractNames?.pascal ?? '',
    contractLabel: contractNames?.label ?? '',
    contractCrate,
    contractIdent: contractCrate.replace(/-/g, '_'),
    shown,
  };
  return { args, names, values };
}

/** What is written where, given the feature's pieces: [piece, template, path in the feature]. */
function pieces(values: Values): [boolean, string, string][] {
  const { slug, pascal, database, migrationFile } = values;
  return [
    [true, 'Cargo.toml', 'Cargo.toml'],
    [true, 'feature.rs', 'feature.rs'],
    [true, 'README.md', 'README.md'],
    [values.backend as boolean, 'backend/mod.rs', 'backend/mod.rs'],
    [values.manifestTest as boolean, 'tests/backend/feature.rs', 'tests/backend/feature.rs'],
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
    [values.people as boolean, 'backend/people.rs', 'backend/people.rs'],
    [Boolean(values.contract), 'contract/Cargo.toml', 'contract/Cargo.toml'],
    [Boolean(values.contract), 'contract/contract.rs', 'contract/contract.rs'],
    [Boolean(values.contract), 'backend/exposed.rs', 'backend/exposed.rs'],
    [Boolean(values.contract), 'tests/backend/exposed.rs', 'tests/backend/exposed.rs'],
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
    [values.connection as boolean, 'frontend/Card.tsx', `frontend/connection/${pascal}Card.tsx`],
    [values.platformSlots as boolean, 'frontend/platform-slots.ts', 'frontend/platform-slots.ts'],
    [values.frontend as boolean, 'tests/browser/browser.spec.ts', `tests/browser/${slug}.spec.ts`],
  ];
}

/** The feature's own files, as its templates fill them. */
export async function renderFeature(root: string, argv: string[], adding = false) {
  const { args, names, values } = featureValues(root, argv, adding);
  const { name } = names;
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
  // Its routes, and who may call each, in its own list, which the app's route test holds it to.
  const routes = [...(values.api ? [`GET ${values.apiPath} Dsp("${name}.view") Read`] : [])];
  if (routes.length)
    plan.files.set(
      `${dir}/tests/api/routes.txt`,
      `# Every route ${name} registers: its method and path, who may call it, the\n` +
        '# database access its work runs under, and whether it wakes the scheduler.\n' +
        '# Each changes with this file, so a reviewer sees who can reach what.\n' +
        `${routes.sort().join('\n')}\n`,
    );
  return { plan, args, names, values };
}

/** Where a page reads what's exposed, whose `ExposedName` names each one a page shows. */
const exposedNames = 'core/shell/frontend/runtime/exposes.ts';
/** `ExposedName` naming `contract` too, as a page shows it in every build. */
export async function showing(root: string, plan: Plan, contract: string) {
  await change(plan, root, exposedNames, (text) =>
    text.replace(
      /export type ExposedName =([^;]+);/,
      (_, names: string) => `export type ExposedName =${names} | '${contract}';`,
    ),
  );
}

export async function planFeature(root: string, argv: string[]) {
  const { plan, args, names, values } = await renderFeature(root, argv);
  const { name } = names;
  const dir = `features/${name}`;

  // List it in app/, as `dispatchdev generate` does from its folder: its crate in the
  // app's Cargo manifest and its manifest in the registry's list. Its manifest brings its
  // routes, and lists its API types itself.
  const edit = (file: string, apply: (text: string) => string) => change(plan, root, file, apply);
  await edit('Cargo.toml', (text) => addWorkspaceMember(text, dir));
  const wired = [...crates(root), crateOf(name, plan.files.get(`${dir}/Cargo.toml`)!)].sort(
    (a, b) => (a.name < b.name ? -1 : 1),
  );
  // The MCP's tools keep the features they need on, as the wiring has them.
  const mcp = needed(tools(root, wired));
  await edit(appManifest, (text) => wiredManifest(text, wired, mcp));
  await edit(featureList, () => wiredList(wired));
  if (values.shown) await showing(root, plan, values.contract as string);

  const declared = [values.switch && 'switch', values.permission && 'permissions'].filter(Boolean);
  const generated = [
    'the feature map',
    ...(values.frontend ? ["the frontend's list"] : []),
    ...(declared.length ? [`the catalog with its ${declared.join(' and ')}`] : []),
    ...(values.api ? [`${dir}/api/generated/`] : []),
  ];
  plan.notes.push(`\`dispatchdev generate\` writes ${generated.join(', and ')}.`);
  plan.notes.push(snapshotsNote);
  if (values.tables && !values.ownDatabase)
    plan.notes.push(
      `Its migration to the ${values.database} database is its own number 1, recorded under its ` +
        "name, apart from every other owner's.",
    );
  if (values.people)
    plan.notes.push(
      `Its people are Driver Match's \`${name}\` kind of data; the app's people test reads them ` +
        'from a new DSP, so its queries are tried without a test of its own.',
    );
  plan.notes.push(`\`npm run test:feature ${name}\` runs its tests of every kind.`);
  return { plan, args };
}

export const generator: Generator = {
  kind: 'feature',
  about: 'a feature: a crate of its own listed in app/, with the pieces its options add',
  usage,
  flags: FLAGS,
  options: OPTIONS,
  plan: planFeature,
  generates: true,
};

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(import.meta.filename))
  await run(generator);
