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
/** Every numbered SQL file of `database`, across the owners, and every recorded migration. */
const migrations = (database: string, at = root) => [
  ...['core', 'collectors', 'features']
    .flatMap((dir) => fs.readdirSync(path.join(at, dir), { recursive: true, encoding: 'utf8' }))
    .filter((name) => new RegExp(`(^|/)migrations/${database}/\\d{4}_[^/]+\\.sql$`).test(name))
    .map((name) => Number(path.basename(name).slice(0, 4))),
  ...(history(at)[database] ?? []).map(({ id }) => id),
];
const historyFile = 'app/tests/rules/migrations-history.json';
const history = (at = root): Record<string, { id: number }[]> =>
  JSON.parse(fs.readFileSync(path.join(at, historyFile), 'utf8'));
const registry = ['app/backend/features.rs', 'app/backend/lib.rs'].find(
  (candidate) =>
    fs.existsSync(candidate) && /pub static REGISTRY\b/.test(fs.readFileSync(candidate, 'utf8')),
)!;

// A copy of the repository's workspace, for what needs a collector that exists only there. Its
// Timecard page draws the tabs other features add to it.
let copy = '';
before(() => {
  copy = temporary();
  for (const entry of [
    'Cargo.toml',
    'Cargo.lock',
    'rust-toolchain.toml',
    'app',
    'core',
    'collectors',
    'features',
    'ops/host-manager',
    'tooling/cli',
    'tooling/shared',
    'tooling/ci/test-plan.json',
    '.github/workflows/checks.yml',
  ])
    fs.cpSync(entry, path.join(copy, entry), {
      recursive: true,
      filter: (source) => !/(^|\/)(node_modules|target)(\/|$)/.test(source),
    });
  fs.writeFileSync(
    path.join(copy, 'features/timecard/frontend/page-tabs.ts'),
    "export const drawn = () => pageTabs('paycom');\n",
  );
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

test('with no flags, a feature is a crate with a switch, a view permission and a browser test', async () => {
  const plan = await feature('parking');
  assert.deepEqual(writes(plan, 'features/parking'), [
    'Cargo.toml',
    'README.md',
    'feature.rs',
    'frontend/feature.ts',
    'frontend/platform-slots.ts',
    'tests/browser/parking.spec.ts',
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
  assert.doesNotMatch(manifest, /mod backend|mod tests/);
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
  // It takes the next free place, after every feature's: last in the sidebar and on the DSPs page.
  const place = Number(/^ {4}place: (\d+),$/m.exec(manifest)?.[1]);
  const places = fs
    .readdirSync('features')
    .filter((name) => fs.existsSync(`features/${name}/feature.rs`))
    .flatMap((name) => [
      ...fs.readFileSync(`features/${name}/feature.rs`, 'utf8').matchAll(/^\s*place: (\d+),/gm),
    ])
    .map((match) => Number(match[1]));
  assert(place % 10 === 0 && places.every((other) => other < place), `${place} follows ${places}`);
  // The DSPs page shows its switch with the icon its frontend gives, which a browser test sees.
  assert.match(
    file(plan, 'features/parking/frontend/feature.ts'),
    /platformSlots: \(\) => import\('.\/platform-slots.js'\),/,
  );
  assert.doesNotMatch(file(plan, 'features/parking/frontend/feature.ts'), /from 'react'|can\(/);
  assert.match(
    file(plan, 'features/parking/frontend/platform-slots.ts'),
    /switch: \{ id: 'parking', icon: LayoutGrid \},/,
  );
  assert.match(
    file(plan, 'features/parking/tests/browser/parking.spec.ts'),
    /getByRole\('switch', \{ name: 'Parking page', exact: true \}\)\)\.toBeChecked\(\)/,
  );

  // The workspace's members cover it already, so Cargo.toml is no change, and the frontend's
  // list is the export's to write.
  assert.deepEqual([...plan.changes.keys()].sort(), [
    'app/backend/Cargo.toml',
    'app/backend/features.rs',
  ]);
  // The registry's list has it by name, there while the app's Cargo feature of its name is on,
  // as it is by default.
  const list = file(plan, 'app/backend/features.rs');
  assert.match(list, /\n {4}#\[cfg\(feature = "parking"\)\]\n {4}&dispatch_parking::FEATURE,\n/);
  const listed = [...list.matchAll(/feature = "(\w+)"\)\]\n {4}&/g)].map((match) => match[1]!);
  assert.deepEqual(listed, [...listed].sort());
  const app = file(plan, 'app/backend/Cargo.toml');
  assert.match(
    app,
    /^dispatch-parking = \{ path = "..\/..\/features\/parking", optional = true \}$/m,
  );
  assert.match(app, /^parking = \["dep:dispatch-parking"\]$/m);
  assert.match(app, /^default = \[[^\]]*"parking",[^\]]*\]/m);
  assert.match(
    plan.changes.get('Cargo.toml') ?? fs.readFileSync('Cargo.toml', 'utf8'),
    /members = \[[^\]]*("features\/\*"|"features\/parking")/,
  );
  // With no switch, it has nothing to show in the frontend.
  assert(
    !writes(await feature('lobby', '--always-on'), 'features/lobby').includes(
      'frontend/feature.ts',
    ),
  );
});

test('--always-on leaves out the switch and, with nothing to gate, the permission', async () => {
  const plan = await feature('lobby', '--always-on');
  const manifest = file(plan, 'features/lobby/feature.rs');
  assert.match(
    manifest,
    /^pub const FEATURE: Feature = Feature \{\n {4}place: \d+,\n {4}\.\.feature\("lobby"\)\n\};$/m,
  );
  assert.doesNotMatch(manifest, /Switch|perm/);
  // With no API or frontend either, it keeps an empty backend/ and its manifest's test: a
  // feature has a backend, an API or a frontend.
  assert(file(plan, 'features/lobby/backend/mod.rs'));
  assert.match(manifest, /#\[cfg\(test\)\]\n#\[path = "tests\/backend\/feature.rs"\]\nmod tests;/);
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
    [
      'api/client.ts',
      'api/mod.rs',
      'api/routes.rs',
      'api/types.rs',
      'tests/api/parking.test.ts',
      'tests/api/routes.txt',
    ],
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
  // It lists its API type itself, which the app's export writes to
  // features/parking/api/generated/, and its root re-exports it.
  assert.match(manifest, /^pub use api::types::ParkingSummary;$/m);
  assert.match(
    manifest,
    /#\[cfg\(feature = "ts"\)\]\npub fn typescript\(cfg: &ts_rs::Config\) -> dispatch_core::Typescript \{\n {4}dispatch_core::typescript!\(cfg, ParkingSummary\)\n\}/,
  );
  assert.match(
    file(plan, 'app/backend/features.rs'),
    /#\[cfg\(feature = "parking"\)\]\n {4}all\.extend\(dispatch_parking::typescript\(cfg\)\);/,
  );
  // Its API types' TypeScript comes behind its ts feature, which the app's tests enable.
  const cargo = file(plan, 'features/parking/Cargo.toml');
  assert.match(cargo, /^\[features\]\n#[^\n]*\nts = \["dep:ts-rs", "dispatch-core\/ts"\]$/m);
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
  // Its manifest brings its routes, and its own list says who may call each: in app/, only its
  // listing changes.
  assert.deepEqual([...plan.changes.keys()].sort(), [
    'app/backend/Cargo.toml',
    'app/backend/features.rs',
  ]);
  assert.match(
    file(plan, 'features/parking/tests/api/routes.txt'),
    /^GET \/api\/dsp\/parking Dsp\("parking.view"\) Read$/m,
  );
});

test('--tables dsp writes the next migration of the DSP database, a storage module and its test', async () => {
  const plan = await feature('parking', '--tables', 'dsp');
  const next = Math.max(...migrations('dsp')) + 1;
  assert(next > Math.max(...history().dsp!.map(({ id }) => id)));
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

test("--keeps tells one collector's collection from another's of the same name", async () => {
  const site = await planCollector(copy, ['yard', '--collection', 'dvic']);
  for (const [name, content] of [...site.plan.files, ...site.plan.changes]) {
    fs.mkdirSync(path.dirname(path.join(copy, name)), { recursive: true });
    fs.writeFileSync(path.join(copy, name), content);
  }
  const { plan } = await planFeature(copy, ['yard_checks', '--keeps', 'yard.dvic']);
  assert.match(
    file(plan, 'features/yard_checks/backend/keeper.rs'),
    /use dispatch_yard::dvic::JOB_KIND;/,
  );
  await assert.rejects(
    planFeature(copy, ['more', '--keeps', 'cortex.dvic']),
    /features\/dvic\/backend\/keeper\.rs keeps cortex\.dvic already/,
  );
});

test('the next migration follows every one declared, as SQL or as code, and every one recorded', async () => {
  const next = (plan: Plan) =>
    Number(/\bid: (\d+),\s*name: "ledger"/.exec(file(plan, 'features/ledger/feature.rs'))?.[1]);
  const shipped = Math.max(...migrations('dsp', copy));
  // A migration that runs code has no file, and one not shipped yet is not in the history.
  const probe = path.join(copy, 'core/db/backend/probe.rs');
  fs.writeFileSync(
    probe,
    `const PROBE: &[Migration] = &[Migration {\n    id: ${shipped + 3},\n    name: "probe",\n` +
      '    apply: Code(probe),\n}];\nconst ALL: &[Migrations] = &[Migrations {\n' +
      '    kind: Kind::DSP,\n    list: PROBE,\n}];\n',
  );
  try {
    assert.equal(next((await planFeature(copy, ['ledger', '--tables', 'dsp'])).plan), shipped + 4);
  } finally {
    fs.rmSync(probe);
  }
  // A shipped migration stays in the history whatever its owner does with its code.
  const recorded = history(copy);
  const file_ = path.join(copy, historyFile);
  const before = fs.readFileSync(file_, 'utf8');
  recorded.dsp = [...recorded.dsp!, { id: shipped + 6 }];
  fs.writeFileSync(file_, JSON.stringify(recorded));
  try {
    assert.equal(next((await planFeature(copy, ['ledger', '--tables', 'dsp'])).plan), shipped + 7);
  } finally {
    fs.writeFileSync(file_, before);
  }
  // A collector's database, named by its own DATABASE constant.
  const cortex = Math.max(...migrations('cortex'));
  assert(
    (await feature('fuel', '--tables', 'cortex')).files.has(
      `features/fuel/migrations/cortex/${String(cortex + 1).padStart(4, '0')}_fuel.sql`,
    ),
  );
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
  // The Agents page offers its kind of data, generated from its read toggle, in a group after
  // every other owner's: the groups follow their kinds' order.
  assert.match(mcp, /missing: "parking",\s*order: \d+,/);
  assert.match(mcp, /reads: &\[PARKING\],\s*missing: "parking data",/);
  const order = Number(/ReadToggle \{[^}]*order: (\d+),/.exec(mcp)?.[1]);
  for (const owner of fs.readdirSync('features')) {
    const other = `features/${owner}/mcp/mod.rs`;
    if (!fs.existsSync(other)) continue;
    for (const toggle of fs
      .readFileSync(other, 'utf8')
      .matchAll(/ReadToggle \{[^}]*order: (\d+),/g))
      assert(Number(toggle[1]) < order, `${other} has the place ${order}`);
  }
  assert.doesNotMatch(file(plan, 'features/parking/frontend/platform-slots.ts'), /readToggles/);
  assert.match(
    file(plan, 'features/parking/tests/mcp/questions.ts'),
    /export function questions\(_world: World\): Question\[\]/,
  );
  assert.match(
    file(plan, 'features/parking/tests/api/routes.txt'),
    /^GET \/api\/v1\/parking Agent\("read"\) Read$/m,
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
      'frontend/platform-slots.ts',
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
  // The export writes the frontend's list from the registry, with the feature in its place.
  assert.match(page.notes.join('\n'), /contracts:generate` writes [^\n]*the frontend's list/);

  const open = file(
    await feature('lobby', '--page', '--always-on'),
    'features/lobby/frontend/feature.ts',
  );
  assert.doesNotMatch(open, /feature: 'lobby'|can\(/);

  // frontend/tabs/ holds the tabs of a feature's own page, so a tab of another's sits beside its
  // manifest. A PageTab has no permission of its own to check.
  const { plan: tab } = await planFeature(copy, ['notes', '--tab-of', 'timecard']);
  assert(tab.files.has('features/notes/frontend/NotesTab.tsx'));
  const tabManifest = file(tab, 'features/notes/frontend/feature.ts');
  assert.match(tabManifest, /pageTabs: \[\s*\{\s*page: 'paycom',/);
  assert.match(tabManifest, /import\('.\/NotesTab.js'\)/);
  assert.doesNotMatch(tabManifest, /visible|permissions\.js/);
  assert.match(file(tab, 'features/notes/tests/browser/notes.spec.ts'), /name: 'Timecard'/);
  assert.match(
    file(tab, 'features/notes/frontend/NotesTab.tsx'),
    /from '..\/..\/..\/core\/shell\/frontend\/ui\/index.js'/,
  );
  // A page that does not draw the tabs other features add would never show it.
  await refused(
    /timecard's page does not draw the tabs other features add to it yet/,
    'notes',
    '--tab-of',
    'timecard',
  );

  const settings = await feature('notes', '--settings');
  assert(settings.files.has('features/notes/frontend/settings/NotesSettings.tsx'));
  assert.match(file(settings, 'features/notes/frontend/feature.ts'), /settingsTabs: \[/);
  assert.match(file(settings, 'features/notes/tests/browser/notes.spec.ts'), /name: 'Settings'/);

  await refused(/choose one/, 'notes', '--page', '--tab-of', 'timecard');
  await refused(/--tab-of names a feature/, 'notes', '--tab-of', 'nowhere');
  await refused(/scorecard has no page/, 'notes', '--tab-of', 'weekly_scorecard');
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
    'frontend/platform-slots.ts',
    'tests/browser/front.spec.ts',
  ]);
  const manifest = file(plan, 'features/front/feature.rs');
  assert.doesNotMatch(manifest, /mod backend|mod tests/);
  assert.match(manifest, /switch: Some\(Switch \{/);
  assert.match(file(plan, 'app/backend/features.rs'), /&dispatch_front::FEATURE,/);
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
    (await planFeature(copy, ['notes', '--tab-of', 'timecard', '--always-on'])).plan,
    await feature('desk', '--mcp'),
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
    'frontend/FleetCard.tsx',
    'frontend/feature.ts',
    'frontend/index.ts',
    'frontend/platform-slots.ts',
    'migrations/fleet/0001_baseline.sql',
    'scripts/auth.js',
    'scripts/records.js',
    'tests/backend/records.rs',
    'tests/browser/fleet.spec.ts',
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
  // Its database holds what core keeps in every collector's: its connection, its identity and
  // a running collection's live results.
  const baseline = file(plan, 'collectors/fleet/migrations/fleet/0001_baseline.sql');
  for (const table of [
    'connections',
    'storage_identity',
    'collection_live_runs',
    'collection_live_items',
  ])
    assert.match(baseline, new RegExp(`CREATE TABLE IF NOT EXISTS ${table} \\(`));
  // Its card is the core's ConnectionCard, in the connectionCard slot; its collection is named
  // for Diagnostics and the audit log, and its capability for the pages that require it, by its
  // manifest, which the labels are generated from.
  const manifest_ = file(plan, 'collectors/fleet/frontend/feature.ts');
  assert.match(manifest_, /connectionCard: \{\s*provider: 'fleet',\s*read,/);
  assert.match(
    manifest_,
    /schedule_fleet_records_required: 'Connect Fleet before enabling Records collections.'/,
  );
  assert.match(file(plan, 'collectors/fleet/frontend/FleetCard.tsx'), /<ConnectionCard\b/);
  assert.match(
    manifest,
    /schedule: "fleet_records",\s*label: "Records",\s*(\/\/[^\n]*\s*)?unit: "row",\s*counted: Counted::Rows,/,
  );
  assert.match(manifest, /Capability \{\s*id: "records",\s*label: "a records source",\s*\}/);
  assert.match(
    file(plan, 'collectors/fleet/frontend/platform-slots.ts'),
    /auditWording: \{ collected: \{ fleet: 'Fleet' \} \},\n\};/,
  );
  assert.match(
    file(plan, 'collectors/fleet/tests/browser/fleet.spec.ts'),
    /getByRole\('button', \{ name: 'Connect Fleet' \}\)/,
  );
  // Core lists job kinds and schedule collections from the registry, so it changes no file there.
  assert.deepEqual(
    [...plan.files.keys(), ...plan.changes.keys()].filter((name) => name.startsWith('core/')),
    [],
  );

  const shards = JSON.parse(file(plan, 'tooling/ci/test-plan.json')).native;
  assert.deepEqual(shards.fleet, ['collectors/fleet/tests/native/fleet-worker.test.ts']);
  assert.match(file(plan, '.github/workflows/checks.yml'), /shard: \[[^\]]*\bfleet, capacity\]/);
  assert.match(
    file(plan, registry),
    /collectors: &\[[^\]]*\n\s+&dispatch_fleet::COLLECTOR,\n\s*\],/,
  );
  // The export writes the frontend's list from the registry's collectors.
  assert.match(plan.notes.join('\n'), /contracts:generate` writes [^\n]*the frontend's list/);
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
  // Schedules name a collection by an id all collectors share, so its own has the site's name.
  assert.match(file(plan, 'collectors/fleet/collector.rs'), /schedule: "fleet_records",/);
  assert.match(
    file(plan, 'collectors/fleet/collector.rs'),
    /unconnected: "schedule_fleet_records_required",/,
  );
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
    // The snapshots of the whole product are rewritten by a command, not by hand.
    assert.match(result.stdout, /`npm run snapshots:update` rewrites the snapshots/);
    assert.doesNotMatch(result.stdout, /catalog\.rs|agent-keys|DISPATCH_UPDATE_/);
    assert(!fs.existsSync('features/parking'), 'a dry run wrote into the repository');
    assert.equal(fs.readFileSync(registry, 'utf8'), before);
    const site = generate('new-collector', ['fleet', '--dry-run']);
    assert.equal(site.status, 0, site.stderr);
    assert(listed(site.stdout, 'Would write').includes('collectors/fleet/collector.rs'));
    assert.match(site.stdout, /`npm run snapshots:update` rewrites the snapshots/);
    assert.doesNotMatch(site.stdout, /catalog\.rs/);
    assert(!fs.existsSync('collectors/fleet'));
  } finally {
    fs.rmSync(out, { recursive: true, force: true });
  }
});

test('without --dry-run it writes into the root, and refuses to write over an owner', () => {
  const wrote = generate('new-feature', ['desk', '--settings', '--root', copy]);
  assert.equal(wrote.status, 0, wrote.stderr);
  assert(fs.existsSync(path.join(copy, 'features/desk/frontend/settings/DeskSettings.tsx')));
  assert.match(
    fs.readFileSync(path.join(copy, 'app/backend/features.rs'), 'utf8'),
    /&dispatch_desk::FEATURE,/,
  );
  // Builds run with --locked: Cargo.lock lists the new crate, and the app's use of it.
  if (spawnSync('cargo', ['--version'], { cwd: copy }).status === 0) {
    assert(listed(wrote.stdout, 'Changed').includes('Cargo.lock'), wrote.stdout);
    const lock = fs.readFileSync(path.join(copy, 'Cargo.lock'), 'utf8');
    assert.match(lock, /\[\[package\]\]\nname = "dispatch-desk"\nversion = "0\.0\.0"\n/);
    assert.match(
      lock,
      /name = "dispatch-backend"\nversion = "0\.0\.0"\ndependencies = \[\n( "[^"]+",\n)* "dispatch-desk",\n/,
    );
  }
  const again = generate('new-feature', ['desk', '--root', copy]);
  assert.equal(again.status, 1);
  assert.match(again.stderr, /desk is taken/);
  const usage = generate('new-feature', ['desk', '--wings']);
  assert.equal(usage.status, 1);
  assert.match(usage.stderr, /Unknown option --wings\n\nUsage: npm run new:feature/);
});
