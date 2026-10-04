import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import * as prettier from 'prettier';
import { planCollector } from '../scaffold/new-collector.js';
import { planFeature } from '../scaffold/new-feature.js';
import { render, repositoryRoot as root, type Plan } from '../scaffold/scaffold.js';

// The generators, run with --dry-run: the files each flag writes, what they say, and how the
// new owner is listed in app/. Nothing here builds Rust; the output is checked as text.

const tsx = path.join(root, 'node_modules/tsx/dist/cli.mjs');
const temporary = () => fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-scaffold-'));
const feature = async (...argv: string[]) => (await planFeature(root, argv)).plan;
const collector = async (...argv: string[]) => (await planCollector(root, argv)).plan;
/** The files a plan writes under `dir`, relative to it. */
const writes = (plan: Plan, dir: string) =>
  [...plan.files.keys()].map((file) => path.posix.relative(dir, file)).sort();
const file = (plan: Plan, name: string) => {
  const content = plan.files.get(name) ?? plan.changes.get(name);
  assert(content !== undefined, `the plan has no ${name}`);
  return content;
};
const refused = async (pattern: RegExp, ...argv: string[]) =>
  assert.rejects(planFeature(root, argv), pattern);
/** Every numbered SQL file of `database`, across the owners. */
const migrations = (database: string) =>
  ['core', 'collectors', 'features']
    .flatMap((dir) => fs.readdirSync(dir, { recursive: true, encoding: 'utf8' }))
    .filter((name) => new RegExp(`(^|/)migrations/${database}/\\d{4}_[^/]+\\.sql$`).test(name))
    .map((name) => Number(path.basename(name).slice(0, 4)));
const registry = ['app/backend/features.rs', 'app/backend/lib.rs'].find(
  (candidate) =>
    fs.existsSync(candidate) && /pub static REGISTRY\b/.test(fs.readFileSync(candidate, 'utf8')),
)!;

// A copy of the repository's owners and app/, for what needs a collector that exists only there.
let copy = '';
before(() => {
  copy = temporary();
  for (const entry of [
    'Cargo.toml',
    'app',
    'core',
    'collectors',
    'features',
    'tooling/ci/test-plan.json',
    '.github/workflows/checks.yml',
  ])
    fs.cpSync(entry, path.join(copy, entry), {
      recursive: true,
      filter: (source) => !/(^|\/)(node_modules|target)(\/|$)/.test(source),
    });
});
after(() => fs.rmSync(copy, { recursive: true, force: true }));

test('templates fill values and keep or drop sections, and refuse a name without a value', () => {
  const source =
    'a {{x}}\n{{#on}}\nshown {{x}}\n{{/on}}\n{{^on}}\nhidden\n{{/on}}\n{{#off}}gone{{/off}}end\n';
  assert.equal(render(source, { x: 1, on: true, off: false }), 'a 1\nshown 1\nend\n');
  assert.equal(render(source, { x: 'y', on: false, off: true }), 'a y\nhidden\ngoneend\n');
  assert.throws(() => render('{{missing}}', {}), /no value for \{\{missing\}\}/);
  assert.throws(() => render('{{#on}}open', { on: true }), /never closed/);
  assert.throws(() => render('{{#on}}x{{/off}}', { on: true, off: true }), /closes on/);
});

test('with no flags, a feature is a backend crate with a switch, a view permission and its first test', async () => {
  const plan = await feature('parking');
  assert.deepEqual(writes(plan, 'features/parking'), [
    'Cargo.toml',
    'README.md',
    'backend/mod.rs',
    'feature.rs',
    'tests/backend/feature.rs',
  ]);
  const cargo = file(plan, 'features/parking/Cargo.toml');
  assert.match(cargo, /^name = "dispatch-parking"$/m);
  // Its package fields are the workspace's, as every crate's are.
  for (const field of ['version', 'edition', 'rust-version', 'publish', 'license'])
    assert.match(cargo, new RegExp(`^${field}\\.workspace = true$`, 'm'));
  assert.match(cargo, /^autotests = false$/m);
  assert.match(cargo, /^\[lib\]\npath = "feature.rs"\ndoctest = false$/m);
  assert.match(cargo, /^\[dependencies\]\ndispatch-core = \{ path = "..\/..\/core" \}\n$/m);
  assert.doesNotMatch(cargo, /dev-dependencies|\[features\]/);

  const manifest = file(plan, 'features/parking/feature.rs');
  assert.match(manifest, /^mod backend;$/m);
  assert.match(manifest, /use dispatch_core::manifest::\{Feature, Switch, feature, perm\};/);
  assert.match(
    manifest,
    /switch: Some\(Switch \{\s*id: "parking",\s*label: "Parking",\s*requires: &\[\],/,
  );
  const order = Number(/perm\("parking.view", "View Parking", (\d+)\)/.exec(manifest)?.[1]);
  const used = fs
    .readdirSync('.', { recursive: true, encoding: 'utf8' })
    .filter((name) => /^(core|collectors|features)\/.*\.rs$/.test(name))
    .flatMap((name) => [
      ...fs.readFileSync(name, 'utf8').matchAll(/\bperm\(\s*"[^"]*",\s*"[^"]*",\s*(\d+)/g),
    ])
    .map((match) => Number(match[1]));
  assert(order % 10 === 0 && used.every((other) => other < order), `${order} follows ${used}`);
  assert.match(manifest, /\.\.feature\("parking"\)\n\};/);
  assert.match(manifest, /#\[cfg\(test\)\]\n#\[path = "tests\/backend\/feature.rs"\]\nmod tests;/);
  assert.match(file(plan, 'features/parking/tests/backend/feature.rs'), /p\.id == "parking.view"/);

  assert.deepEqual(
    [...plan.changes.keys()].sort(),
    ['Cargo.toml', 'app/backend/Cargo.toml', registry].sort(),
  );
  // The registry's last feature, there while the app's Cargo feature of its name is on, as
  // it is by default.
  assert.match(
    file(plan, registry),
    /features: &\[\n(\s+#\[cfg\(feature = "\w+"\)\]\n\s+&\w+::FEATURE,\n)*\s+#\[cfg\(feature = "parking"\)\]\n\s+&dispatch_parking::FEATURE,\n\s*\],/,
  );
  const app = file(plan, 'app/backend/Cargo.toml');
  assert.match(
    app,
    /^dispatch-parking = \{ path = "..\/..\/features\/parking", optional = true \}$/m,
  );
  assert.match(app, /^parking = \["dep:dispatch-parking"\]$/m);
  assert.match(app, /^default = \[[^\]]*"parking",?\s*\]/m);
  assert.match(file(plan, 'Cargo.toml'), /members = \[[^\]]*("features\/\*"|"features\/parking")/);
});

test('--always-on leaves out the switch and, with nothing to gate, the permission', async () => {
  const plan = await feature('lobby', '--always-on');
  const manifest = file(plan, 'features/lobby/feature.rs');
  assert.match(manifest, /^pub const FEATURE: Feature = feature\("lobby"\);$/m);
  assert.doesNotMatch(manifest, /Switch|perm/);
  assert.match(
    file(plan, 'features/lobby/tests/backend/feature.rs'),
    /FEATURE\.switch\.is_none\(\)/,
  );
  assert.match(file(plan, 'features/lobby/README.md'), /Always on/);
  // An endpoint still needs a permission to check.
  const gated = file(await feature('lobby', '--always-on', '--api'), 'features/lobby/feature.rs');
  assert.doesNotMatch(gated, /switch:/);
  assert.match(gated, /perm\("lobby.view", "View Lobby", \d+\)/);
});

test('--api writes one endpoint behind the view permission, its client function and an API test', async () => {
  const plan = await feature('parking', '--api');
  assert.deepEqual(
    writes(plan, 'features/parking').filter((name) => /^(api|tests\/api)\//.test(name)),
    ['api/client.ts', 'api/mod.rs', 'api/routes.rs', 'api/types.rs', 'tests/api/parking.test.ts'],
  );
  const routes = file(plan, 'features/parking/api/routes.rs');
  assert.match(routes, /const VIEW: Dsp = Dsp\("parking.view"\);/);
  assert.match(routes, /vec!\[read\("\/api\/dsp\/parking", VIEW, summary\)\]/);
  assert.match(
    file(plan, 'features/parking/api/types.rs'),
    /ts\(export_to = "features\/parking\/api\/generated\/"\)/,
  );
  const client = file(plan, 'features/parking/api/client.ts');
  assert.match(client, /from '.\/generated\/ParkingSummary.js'/);
  assert.match(
    client,
    /export const useParkingSummary = \(\) => useData<ParkingSummary>\(parkingSummaryUrl\);/,
  );
  const apiTest = file(plan, 'features/parking/tests/api/parking.test.ts');
  assert.match(apiTest, /from '..\/..\/..\/..\/core\/shell\/tests\/support\/support.js'/);
  assert.match(apiTest, /assert\.equal\(\(await member\.get\(route\)\)\.status, 403\);/);
  const manifest = file(plan, 'features/parking/feature.rs');
  assert.match(manifest, /^mod api;$/m);
  assert.match(manifest, /^    routes: api::routes::routes,$/m);
  // The app's export test lists its API type, which its root re-exports, and writes it to
  // features/parking/api/generated/.
  assert.match(manifest, /^pub use api::types::ParkingSummary;$/m);
  assert.match(
    file(plan, 'app/tests/backend/export.rs'),
    /\n    #\[cfg\(feature = "parking"\)\]\n    bindings\.extend\(exported!\(&cfg, dispatch_parking::ParkingSummary\)\);\n    bindings\.insert\(ACCESS_CATALOG/,
  );
  // Its API types' TypeScript comes behind its ts feature, which the app's tests enable.
  const cargo = file(plan, 'features/parking/Cargo.toml');
  assert.match(cargo, /^\[features\]\n#[^\n]*\nts = \["dep:ts-rs"\]$/m);
  assert.match(cargo, /^ts-rs = \{ workspace = true, optional = true \}$/m);
  assert.doesNotMatch(cargo, /dev-dependencies/);
  assert.match(
    file(plan, 'features/parking/api/types.rs'),
    /^#\[cfg_attr\(feature = "ts", derive\(ts_rs::TS\)\)\]$/m,
  );
  const app = file(plan, 'app/backend/Cargo.toml');
  assert.match(
    app.slice(app.indexOf('\n[dev-dependencies]\n')).split(/\n\[/)[1]!,
    /^dispatch-parking = \{ path = "..\/..\/features\/parking", features = \["ts"\] \}$/m,
  );
  // Its manifest brings its routes: in app/, only its listing, the inventory and the export
  // change.
  assert.deepEqual(
    [...plan.changes.keys()].sort(),
    [
      'Cargo.toml',
      'app/backend/Cargo.toml',
      registry,
      'app/tests/backend/integration/http_routes.rs',
      'app/tests/backend/export.rs',
    ].sort(),
  );
  assert.match(
    file(plan, 'app/tests/backend/integration/http_routes.rs'),
    /\("GET", "\/api\/dsp\/parking", Dsp\("parking.view"\), Read, false\),\n\];/,
  );
});

test('--tables dsp writes the next migration of the DSP database, a storage module and its test', async () => {
  const plan = await feature('parking', '--tables', 'dsp');
  const next = Math.max(...migrations('dsp')) + 1;
  const sql = `migrations/dsp/${String(next).padStart(4, '0')}_parking.sql`;
  assert.deepEqual(
    writes(plan, 'features/parking').filter((name) =>
      /^(migrations|backend\/storage|tests\/backend\/storage)/.test(name),
    ),
    ['backend/storage.rs', sql, 'tests/backend/storage.rs'].sort(),
  );
  assert.match(
    file(plan, `features/parking/${sql}`),
    /CREATE TABLE IF NOT EXISTS parking_items \(/,
  );
  const manifest = file(plan, 'features/parking/feature.rs');
  assert.match(manifest, /tables: &\[\("dsp", &\["parking_items"\]\)\],/);
  assert.match(manifest, /kind: Kind::DSP,/);
  assert.match(
    manifest,
    new RegExp(`id: ${next},\\s*name: "parking",\\s*apply: Sql\\(include_str!\\("${sql}"\\)\\)`),
  );
  assert.match(manifest, /^pub use backend::storage::\{ParkingItem, ParkingStore\};$/m);
  const storage = file(plan, 'features/parking/backend/storage.rs');
  assert.match(storage, /impl ParkingStore for Store \{/);
  assert.match(storage, /let db = self\.dsp\(dsp\)\?;/);
  assert.match(storage, /#\[path = "..\/tests\/backend\/storage.rs"\]\nmod tests;/);
  assert.match(
    file(plan, 'features/parking/tests/backend/storage.rs'),
    /testing::install\(&\[\], &\[&crate::FEATURE\]\);/,
  );
  const cargo = file(plan, 'features/parking/Cargo.toml');
  assert.match(cargo, /^rusqlite = \{ workspace = true \}$/m);
  assert.match(cargo, /^dispatch-core = \{ path = "..\/..\/core", features = \["testing"\] \}$/m);
});

test("--tables <collector> stores in the collector's database and depends on its crate", async () => {
  const plan = await feature('fuel', '--tables', 'cortex');
  const next = Math.max(...migrations('cortex')) + 1;
  assert(
    plan.files.has(`features/fuel/migrations/cortex/${String(next).padStart(4, '0')}_fuel.sql`),
  );
  assert.match(file(plan, 'features/fuel/feature.rs'), /kind: dispatch_cortex::DATABASE,/);
  assert.match(
    file(plan, 'features/fuel/backend/storage.rs'),
    /self\.collector\(dsp, dispatch_cortex::PROVIDER\)\?/,
  );
  assert.match(
    file(plan, 'features/fuel/Cargo.toml'),
    /^dispatch-cortex = \{ path = "..\/..\/collectors\/cortex" \}$/m,
  );
  assert.match(
    file(plan, 'features/fuel/tests/backend/storage.rs'),
    /install\(&\[&dispatch_cortex::COLLECTOR\]/,
  );
  await refused(/--tables takes dsp/, 'fuel', '--tables', 'routedata');
  await refused(/--tables takes dsp/, 'fuel', '--tables', 'fuel');
});

test('--keeps refuses a collection another feature keeps, or one that does not exist', async () => {
  await refused(
    /features\/dvic\/backend\/keeper\.rs keeps cortex\.dvic already/,
    'more',
    '--keeps',
    'cortex.dvic',
  );
  await refused(/cortex collects .*, not nothing/, 'more', '--keeps', 'cortex.nothing');
  await refused(/--keeps names a collector/, 'more', '--keeps', 'nowhere.records');
});

test("--keeps writes the keeper of a collector's collection, tested with its fixture", async () => {
  // A collector of its own, written into the copy, has a collection no feature keeps yet.
  const site = await planCollector(copy, ['fleet', '--collection', 'shifts']);
  for (const [name, content] of [...site.plan.files, ...site.plan.changes]) {
    fs.mkdirSync(path.dirname(path.join(copy, name)), { recursive: true });
    fs.writeFileSync(path.join(copy, name), content);
  }
  const { plan } = await planFeature(copy, [
    'fleet_log',
    '--keeps',
    'fleet.shifts',
    '--tables',
    'fleet_log',
  ]);
  assert.deepEqual(
    writes(plan, 'features/fleet_log').filter((name) => /keeper|storage|migrations/.test(name)),
    [
      'backend/keeper.rs',
      'backend/storage.rs',
      'migrations/fleet_log/0001_baseline.sql',
      'tests/backend/keeper.rs',
      'tests/backend/storage.rs',
    ],
  );
  const manifest = file(plan, 'features/fleet_log/feature.rs');
  assert.match(manifest, /requires: &\["shifts"\],/);
  assert.match(
    manifest,
    /perm\("fleet_log.collect", "Collect Fleet Log", \d+\)\.implies\(&\["fleet_log.view"\]\)/,
  );
  assert.match(manifest, /keeps: &\[&backend::keeper::FleetLogKeeper\],/);
  assert.match(
    manifest,
    /kind: backend::storage::DATABASE,\s*list: &\[Migration \{\s*id: 1,\s*name: "baseline",/,
  );
  const keeper = file(plan, 'features/fleet_log/backend/keeper.rs');
  assert.match(keeper, /use dispatch_fleet::shifts::JOB_KIND;/);
  assert.match(keeper, /"fleet_log.collect"/);
  assert.match(keeper, /fn storages\(&self\)/);
  assert.match(
    file(plan, 'features/fleet_log/tests/backend/keeper.rs'),
    /dispatch_fleet::COLLECTOR\s*\.fixture\(/,
  );
  assert.match(
    file(plan, 'features/fleet_log/migrations/fleet_log/0001_baseline.sql'),
    /storage_identity/,
  );
  assert.match(
    file(plan, 'features/fleet_log/backend/storage.rs'),
    /self\.added_storage\(dsp, dispatch_fleet::PROVIDER, &STORAGE\)\?/,
  );
  assert.match(
    file(plan, 'features/fleet_log/Cargo.toml'),
    /^dispatch-fleet = \{ path = "..\/..\/collectors\/fleet" \}$/m,
  );
  // A database of its own sits beside the collector whose collection it keeps.
  await assert.rejects(
    planFeature(copy, ['fleet_log', '--tables', 'fleet_log']),
    /--tables takes dsp/,
  );
});

test('--mcp writes one agent endpoint, its read toggle and an eval question', async () => {
  const plan = await feature('parking', '--mcp');
  assert.deepEqual(
    writes(plan, 'features/parking').filter((name) => /mcp\//.test(name)),
    ['mcp/catalog.rs', 'mcp/mod.rs', 'mcp/views.rs', 'tests/mcp/questions.ts'],
  );
  assert.match(file(plan, 'features/parking/feature.rs'), /mcp: mcp::MCP,/);
  const mcp = file(plan, 'features/parking/mcp/mod.rs');
  assert.match(
    mcp,
    /pub\(crate\) const PARKING: AgentArea = AgentArea::new\(&ReadToggle \{\s*id: "parking",/,
  );
  assert.match(mcp, /features: &\["parking"\],/);
  assert.match(file(plan, 'features/parking/mcp/catalog.rs'), /path: "\/api\/v1\/parking",/);
  assert.match(
    file(plan, 'features/parking/tests/mcp/questions.ts'),
    /export function questions\(_world: World\): Question\[\]/,
  );
  assert.match(
    file(plan, 'app/tests/backend/integration/http_routes.rs'),
    /\("GET", "\/api\/v1\/parking", Agent\("read"\), Read, false\),/,
  );
  await refused(/--mcp needs a feature with one/, 'parking', '--mcp', '--always-on');
});

test('--page, --tab-of and --settings write frontend/feature.ts, the screen and a browser test', async () => {
  const page = await feature('parking', '--page', '--api');
  assert.deepEqual(
    writes(page, 'features/parking').filter((name) => /frontend|browser/.test(name)),
    [
      'frontend/ParkingPage.tsx',
      'frontend/feature.ts',
      'frontend/index.ts',
      'tests/browser/parking.spec.ts',
    ],
  );
  const manifest = file(page, 'features/parking/frontend/feature.ts');
  assert.match(
    manifest,
    /feature: 'parking',\n\s*permission: \(\{ view \}\) => can\(view, 'parking.view'\),/,
  );
  assert.match(manifest, /warm\(\[parkingSummaryUrl\]\)/);
  assert.match(file(page, 'features/parking/frontend/ParkingPage.tsx'), /useParkingSummary\(\)/);
  assert.match(
    file(page, 'features/parking/tests/browser/parking.spec.ts'),
    /getByRole\('link', \{ name: 'Parking', exact: true \}\)/,
  );
  const list = file(page, 'app/frontend/features.ts');
  assert.match(
    list,
    /^import \{ feature as parking \} from '..\/..\/features\/parking\/frontend\/feature.js';$/m,
  );
  assert(
    list.indexOf('  parking,') < list.indexOf('  accounts,'),
    "features come before core's parts",
  );

  const open = file(
    await feature('lobby', '--page', '--always-on'),
    'features/lobby/frontend/feature.ts',
  );
  assert.doesNotMatch(open, /feature: 'lobby'|can\(/);

  const tab = await feature('notes', '--tab-of', 'timecard');
  assert(tab.files.has('features/notes/frontend/tabs/notes/NotesTab.tsx'));
  assert.match(
    file(tab, 'features/notes/frontend/feature.ts'),
    /pageTabs: \[\s*\{\s*page: 'paycom',/,
  );
  assert.match(file(tab, 'features/notes/tests/browser/notes.spec.ts'), /name: 'Timecard'/);
  assert.match(
    file(tab, 'features/notes/frontend/tabs/notes/NotesTab.tsx'),
    /from '..\/..\/..\/..\/..\/core\/shell\/frontend\/ui\/index.js'/,
  );

  const settings = await feature('notes', '--settings');
  assert(settings.files.has('features/notes/frontend/settings/NotesSettings.tsx'));
  assert.match(file(settings, 'features/notes/frontend/feature.ts'), /settingsTabs: \[/);
  assert.match(file(settings, 'features/notes/tests/browser/notes.spec.ts'), /name: 'Settings'/);

  await refused(/choose one/, 'notes', '--page', '--tab-of', 'timecard');
  await refused(/--tab-of names a feature/, 'notes', '--tab-of', 'nowhere');
  await refused(/scorecard has no page/, 'notes', '--tab-of', 'scorecard');
});

test('--no-backend writes a frontend-only feature, and refuses the pieces that need Rust logic', async () => {
  const plan = await feature('front', '--page', '--no-backend');
  assert.deepEqual(writes(plan, 'features/front'), [
    'Cargo.toml',
    'README.md',
    'feature.rs',
    'frontend/FrontPage.tsx',
    'frontend/feature.ts',
    'frontend/index.ts',
    'tests/browser/front.spec.ts',
  ]);
  const manifest = file(plan, 'features/front/feature.rs');
  assert.doesNotMatch(manifest, /mod backend|mod tests/);
  assert.match(manifest, /switch: Some\(Switch \{/);
  assert.match(file(plan, registry), /&dispatch_front::FEATURE,/);
  assert.match(file(plan, 'app/frontend/features.ts'), /feature as front/);
  assert(
    writes(await feature('front', '--settings', '--no-backend'), 'features/front').includes(
      'frontend/settings/FrontSettings.tsx',
    ),
  );
  await refused(/add --page, --tab-of or --settings/, 'front', '--no-backend');
  for (const piece of [['--api'], ['--tables', 'dsp'], ['--keeps', 'cortex.dvic'], ['--mcp']])
    await refused(
      new RegExp(`Rust logic that ${piece[0]} needs`),
      'front',
      '--page',
      '--no-backend',
      ...piece,
    );
});

test('names are lowercase words, not taken by an owner and not retired', async () => {
  await refused(/is not a name/, 'Parking');
  await refused(/is not a name/, 'parking-lot');
  for (const taken of ['uniforms', 'cortex', 'accounts']) await refused(/is taken/, taken);
  await refused(/retired or reserved/, 'workforce');
  await assert.rejects(planCollector(root, ['dvic']), /is taken/);
});

test('everything written is formatted as the repository formats it, with no placeholder left', async () => {
  const plans = [
    await feature(
      'parking_lot',
      '--api',
      '--tables',
      'dsp',
      '--mcp',
      '--page',
      '--settings',
      '--label',
      'Parking Lot',
    ),
    await feature('notes', '--tab-of', 'dvic', '--always-on'),
    await collector('fleet'),
  ];
  const config = (await prettier.resolveConfig(path.join(root, 'package.json'))) ?? {};
  for (const plan of plans)
    for (const [name, content] of [...plan.files, ...plan.changes]) {
      assert.doesNotMatch(content, /\{\{[#^/]?\w+\}\}/, `${name} keeps a placeholder`);
      if (/\.([jt]sx?|md|json|ya?ml)$/.test(name))
        assert(
          await prettier.check(content, { ...config, filepath: name }),
          `${name} is not formatted`,
        );
    }
  // Where rustfmt runs, the new Rust is as `cargo fmt` leaves it.
  if (spawnSync('rustfmt', ['--version'], { cwd: root }).status !== 0) return;
  const written = temporary();
  try {
    const rust = plans.flatMap((plan) => [...plan.files].filter(([name]) => name.endsWith('.rs')));
    for (const [name, content] of rust) {
      fs.mkdirSync(path.dirname(path.join(written, name)), { recursive: true });
      fs.writeFileSync(path.join(written, name), content);
    }
    const check = spawnSync(
      'rustfmt',
      ['--edition', '2024', '--check', ...rust.map(([name]) => path.join(written, name))],
      { cwd: root, encoding: 'utf8' },
    );
    assert.equal(check.status, 0, check.stdout + check.stderr);
  } finally {
    fs.rmSync(written, { recursive: true, force: true });
  }
});

test('new:collector writes its connection, one collection with a fixture, a card and a native test in a shard of its own', async () => {
  const plan = await collector('fleet', '--host', 'portal.fleet.test');
  assert.deepEqual(writes(plan, 'collectors/fleet'), [
    'Cargo.toml',
    'README.md',
    'collections/mod.rs',
    'collections/records/mod.rs',
    'collector.rs',
    'connection/mod.rs',
    'fixtures/mod.rs',
    'fixtures/records.rs',
    'frontend/feature.ts',
    'migrations/fleet/0001_baseline.sql',
    'scripts/auth.js',
    'scripts/records.js',
    'tests/backend/records.rs',
    'tests/native/fleet-worker.test.ts',
  ]);
  const cargo = file(plan, 'collectors/fleet/Cargo.toml');
  assert.match(cargo, /^name = "dispatch-fleet"$/m);
  assert.match(cargo, /^path = "collector.rs"\ndoctest = false$/m);
  assert.match(cargo, /^version\.workspace = true$/m);
  assert.match(cargo, /^operator-probes = \[\]$/m);
  const manifest = file(plan, 'collectors/fleet/collector.rs');
  assert.match(manifest, /^mod collections;$/m);
  assert.match(manifest, /^pub use collections::records;$/m);
  assert.match(manifest, /pub const PROVIDER: Provider = Provider::new\("fleet"\);/);
  assert.match(manifest, /pub const HOST: &str = "portal.fleet.test";/);
  assert.match(manifest, /job_kind: collections::records::JOB_KIND,/);
  assert.match(
    file(plan, 'collectors/fleet/collections/records/mod.rs'),
    /pub const JOB_KIND: &str = "fleet.records.collect";/,
  );
  assert.match(
    file(plan, 'collectors/fleet/connection/mod.rs'),
    /include_str!\("..\/scripts\/auth.js"\)/,
  );
  assert.match(
    file(plan, 'collectors/fleet/tests/native/fleet-worker.test.ts'),
    /process\.env\.DISPATCH_TEST_NATIVE !== '1'/,
  );

  const shards = JSON.parse(file(plan, 'tooling/ci/test-plan.json')).native;
  assert.deepEqual(shards.fleet, ['collectors/fleet/tests/native/fleet-worker.test.ts']);
  assert.match(
    file(plan, '.github/workflows/checks.yml'),
    /shard: \[[^\]]*cortex-meals, fleet, capacity\]/,
  );
  assert.match(
    file(plan, registry),
    /collectors: &\[[^\]]*\n\s+&dispatch_fleet::COLLECTOR,\n\s*\],/,
  );
  assert.match(
    file(plan, 'app/frontend/features.ts'),
    /feature as fleet \} from '..\/..\/collectors\/fleet\/frontend\/feature.js'/,
  );
  assert.match(
    file(plan, 'app/backend/Cargo.toml'),
    /^dispatch-fleet = \{ path = "..\/..\/collectors\/fleet" \}$/m,
  );
  assert.match(
    file(plan, 'app/backend/Cargo.toml'),
    /^operator-probes = \[[^\]]*"dispatch-fleet\/operator-probes"[^\]]*\]$/m,
  );

  const named = await collector('fleet', '--collection', 'shifts');
  assert(named.files.has('collectors/fleet/collections/shifts/mod.rs'));
  assert(named.files.has('collectors/fleet/tests/backend/shifts.rs'));
  await assert.rejects(
    planCollector(root, ['fleet', '--host', 'not a host']),
    /is not a host name/,
  );
});

/** Runs a generator as `npm run` would. */
const generate = (generator: string, args: string[]) =>
  spawnSync(process.execPath, [tsx, path.join(root, `tooling/scaffold/${generator}.ts`), ...args], {
    cwd: root,
    encoding: 'utf8',
  });
const listed = (stdout: string, heading: string) =>
  stdout
    .split(`${heading}:\n`)[1]!
    .split('\n')
    .filter((line) => line.startsWith('  ') && !line.startsWith('  - '))
    .map((line) => line.trim());

test('--dry-run prints what it would write and changes nothing; --out receives it whole', () => {
  const before = fs.readFileSync(registry, 'utf8');
  const out = temporary();
  try {
    const result = generate('new-feature', [
      'parking',
      '--api',
      '--page',
      '--dry-run',
      '--out',
      out,
    ]);
    assert.equal(result.status, 0, result.stderr);
    const files = listed(result.stdout, 'Would write');
    assert(
      files.includes('features/parking/feature.rs') &&
        files.includes('features/parking/api/client.ts'),
    );
    for (const name of [...files, ...listed(result.stdout, 'Would change')])
      assert(fs.existsSync(path.join(out, name)), `${name} is not under --out`);
    assert.match(result.stdout, /^Next:\n/m);
    assert(!fs.existsSync('features/parking'), 'a dry run wrote into the repository');
    assert.equal(fs.readFileSync(registry, 'utf8'), before);
    const site = generate('new-collector', ['fleet', '--dry-run']);
    assert.equal(site.status, 0, site.stderr);
    assert(listed(site.stdout, 'Would write').includes('collectors/fleet/collector.rs'));
    assert(!fs.existsSync('collectors/fleet'));
  } finally {
    fs.rmSync(out, { recursive: true, force: true });
  }
});

test('without --dry-run it writes into the root, and refuses to write over an owner', () => {
  const wrote = generate('new-feature', ['desk', '--settings', '--root', copy]);
  assert.equal(wrote.status, 0, wrote.stderr);
  assert(fs.existsSync(path.join(copy, 'features/desk/frontend/settings/DeskSettings.tsx')));
  assert.match(fs.readFileSync(path.join(copy, registry), 'utf8'), /&dispatch_desk::FEATURE,/);
  const again = generate('new-feature', ['desk', '--root', copy]);
  assert.equal(again.status, 1);
  assert.match(again.stderr, /desk is taken/);
  const usage = generate('new-feature', ['desk', '--wings']);
  assert.equal(usage.status, 1);
  assert.match(usage.stderr, /Unknown option --wings\n\nUsage: npm run new:feature/);
});
