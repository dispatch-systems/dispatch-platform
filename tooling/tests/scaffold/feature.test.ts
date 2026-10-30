import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { planCollector } from '../../scaffold/new-collector.js';
import { planFeature, withPreset } from '../../scaffold/new-feature.js';
import { render } from '../../scaffold/scaffold.js';
import { copyWorkspace, feature, file, refused, root, writes } from './support.js';

// new feature: what each flag writes, and the names and flags it refuses.

// A copy of the repository's workspace, which these tests write into.
let copy = '';
before(() => {
  copy = copyWorkspace();
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
  assert.match(manifest, /use dispatch_core::manifest::\{Feature, feature, optional, perm\};/);
  assert.match(manifest, /switch: optional\("parking", "Parking", &\[\]\),/);
  const order = Number(/perm\("parking.view", "View Parking", (\d+)\)/.exec(manifest)?.[1]);
  // Only the source roots: a build writing to target/ meanwhile would pull names from under a
  // walk of the whole checkout.
  const used = ['core', 'collectors', 'features']
    .flatMap((dir) =>
      fs.readdirSync(dir, { recursive: true, encoding: 'utf8' }).map((name) => `${dir}/${name}`),
    )
    .filter((name) => name.endsWith('.rs'))
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
    !writes(await feature('lobby', '--mandatory'), 'features/lobby').includes(
      'frontend/feature.ts',
    ),
  );
});

test('--mandatory makes a feature every DSP has and, with nothing to gate, no permission', async () => {
  const plan = await feature('lobby', '--mandatory');
  const manifest = file(plan, 'features/lobby/feature.rs');
  assert.match(
    manifest,
    /^pub const FEATURE: Feature = Feature \{\n {4}place: \d+,\n {4}switch: mandatory\("lobby", "Lobby"\),\n {4}\.\.feature\("lobby"\)\n\};$/m,
  );
  assert.doesNotMatch(manifest, /optional|perm/);
  // With no API or frontend either, it keeps an empty backend/ and its manifest's test: a
  // feature has a backend, an API or a frontend.
  assert(file(plan, 'features/lobby/backend/mod.rs'));
  assert.match(manifest, /#\[cfg\(test\)\]\n#\[path = "tests\/backend\/feature.rs"\]\nmod tests;/);
  assert.match(file(plan, 'features/lobby/tests/backend/feature.rs'), /assert!\(switch\.mandatory/);
  assert.match(file(plan, 'features/lobby/README.md'), /Mandatory/);
  // An endpoint still needs a permission to check.
  const gated = file(await feature('lobby', '--mandatory', '--api'), 'features/lobby/feature.rs');
  assert.match(gated, /switch: mandatory\("lobby", "Lobby"\),/);
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
  assert.match(manifest, /^ {4}routes: api::routes::routes,$/m);
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

test('--tables dsp writes its own first migration of the DSP database, a storage module and its test', async () => {
  const plan = await feature('parking', '--tables', 'dsp');
  // Numbered by the feature itself, whatever other owners added to the database.
  const sql = 'migrations/dsp/0001_parking.sql';
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
  assert.match(manifest, /own_migrations: &\[OwnMigrations \{\s*kind: Kind::DSP,/);
  assert.match(
    manifest,
    new RegExp(`id: 1,\\s*name: "parking",\\s*apply: Sql\\(include_str!\\("${sql}"\\)\\)`),
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
  assert(plan.files.has('features/fuel/migrations/cortex/0001_fuel.sql'));
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

test('--people connects its table to Driver Match by the IDs of a source, and needs --tables', async () => {
  const plan = await feature('fuel', '--tables', 'dsp', '--people', 'amazon');
  const people = file(plan, 'features/fuel/backend/people.rs');
  assert.match(people, /^pub const PEOPLE: People = People::new\($/m);
  assert.match(people, /^\s+dispatch_cortex::SOURCE,$/m);
  assert.match(people, /&\*store\.dsp\(dsp\)\?,/);
  assert.match(file(plan, 'features/fuel/feature.rs'), /people: &\[&backend::people::PEOPLE\],/);
  assert.match(file(plan, 'features/fuel/backend/mod.rs'), /^pub\(crate\) mod people;$/m);
  assert.match(
    file(plan, 'features/fuel/Cargo.toml'),
    /^dispatch-cortex = \{ path = "..\/..\/collectors\/cortex" \}$/m,
  );
  // The crate it uses for its people is one it declares, as the dependency rule holds it to.
  assert.match(file(plan, 'features/fuel/feature.rs'), /^\s+depends_on: &\["cortex"\],$/m);
  await refused(/--people reads who it names from its table/, 'fuel', '--people', 'amazon');
  await refused(
    /--people names a source of IDs: one of amazon, paycom/,
    'fuel',
    '--tables',
    'dsp',
    '--people',
    'slack',
  );
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
  assert.match(manifest, /switch: optional\("fleet_log", "Fleet Log", &\["shifts"\]\),/);
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
  // What it collects is under the schedule its collector sets, counted as its collector counts.
  assert.match(file(plan, 'features/fleet_log/frontend/feature.ts'), /collected: \(\) => import/);
  assert.match(
    file(plan, 'features/fleet_log/frontend/collected.ts'),
    /^ {2}fleet_shifts: \{[^]*counted: 'rows'/m,
  );
  // A database of its own sits beside the collector whose collection it keeps.
  await assert.rejects(
    planFeature(copy, ['fleet_log', '--tables', 'fleet_log']),
    /--tables takes dsp/,
  );
  // What it collects is said in its frontend, which a mandatory feature with no screen lacks.
  await assert.rejects(
    planFeature(copy, ['fleet_log', '--keeps', 'fleet.shifts', '--mandatory']),
    /add --page, or leave out --mandatory/,
  );
});

test('--page and --tab-of write frontend/feature.ts, the screen and a browser test', async () => {
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
  assert.match(page.notes.join('\n'), /`dispatchdev generate` writes [^\n]*the frontend's list/);
  // Once the dev server restarts on its backend, a screenshot shows its page.
  assert.match(
    page.notes.join('\n'),
    /Restart `npm run dev` for its backend; then `dispatchdev shot --dsp dev --page parking` shows its page\./,
  );

  const open = file(
    await feature('lobby', '--page', '--mandatory'),
    'features/lobby/frontend/feature.ts',
  );
  assert.doesNotMatch(open, /feature: 'lobby'|can\(/);

  // frontend/tabs/ holds the tabs of a feature's own page, so a tab of another's sits beside its
  // manifest. A PageTab has no permission of its own to check.
  const tab = await feature('notes', '--tab-of', 'timecard');
  assert(tab.files.has('features/notes/frontend/NotesTab.tsx'));
  const tabManifest = file(tab, 'features/notes/frontend/feature.ts');
  assert.match(tabManifest, /pageTabs: \[\s*\{\s*page: 'paycom',/);
  assert.match(tabManifest, /import\('.\/NotesTab.js'\)/);
  assert.doesNotMatch(tabManifest, /visible|permissions\.js/);
  assert.match(file(tab, 'features/notes/tests/browser/notes.spec.ts'), /name: 'Timecard'/);
  assert.match(
    tab.notes.join('\n'),
    /`dispatchdev shot --dsp dev --page paycom --tab notes` shows its tab of Timecard/,
  );
  assert.match(
    file(tab, 'features/notes/frontend/NotesTab.tsx'),
    /from '..\/..\/..\/core\/accounts\/api\/index.js'/,
  );
  // Every page draws the tabs other features add to it, one without tabs of its own too.
  const uniforms = file(
    await feature('notes', '--tab-of', 'uniforms'),
    'features/notes/frontend/feature.ts',
  );
  assert.match(uniforms, /pageTabs: \[\s*\{\s*page: 'uniforms',/);
  await refused(/use --settings/, 'notes', '--tab-of', 'settings');

  await refused(
    /--page and --tab-of each give it its own screen/,
    'notes',
    '--page',
    '--tab-of',
    'timecard',
  );
  await refused(/--tabs and --tab-of/, 'notes', '--tabs', 'a,b', '--tab-of', 'timecard');
  await refused(/--tab-of names a feature/, 'notes', '--tab-of', 'nowhere');
  await refused(/scorecard has no page/, 'notes', '--tab-of', 'weekly_scorecard');
});

test('--settings declares a section its backend reads, which Settings draws: no screen of its own', async () => {
  const plan = await feature('notes', '--settings', '--icon', 'Truck');
  // Its frontend is its switch's alone, and so is its browser test.
  assert.deepEqual(
    writes(plan, 'features/notes').filter((name) => /^(backend|frontend|tests)\//.test(name)),
    [
      'backend/mod.rs',
      'backend/settings.rs',
      'frontend/feature.ts',
      'frontend/platform-slots.ts',
      'tests/backend/settings.rs',
      'tests/browser/notes.spec.ts',
    ],
  );
  const frontend = file(plan, 'features/notes/frontend/feature.ts');
  assert.doesNotMatch(frontend, /settingsSections/);
  assert.doesNotMatch(file(plan, 'features/notes/tests/browser/notes.spec.ts'), /'Settings'/);
  assert.match(file(plan, 'features/notes/frontend/platform-slots.ts'), /icon: Truck \}/);
  // With no page of its own, its manifest gives the icon its page of Settings shows.
  assert.match(frontend, /^import \{ Truck \} from 'lucide-react';$/m);
  assert.match(frontend, /^ {2}\/\/ The icon its page of Settings shows\.\n {2}icon: Truck,$/m);
  assert.match(
    plan.notes.join('\n'),
    /`dispatchdev shot --dsp dev --page settings --tab notes` shows its page of Settings\./,
  );
  // Its page gives it, once its switch gates the page; else its manifest still does.
  const manifestIcon = /^ {2}icon: /m;
  const paged = file(
    await feature('notes', '--page', '--settings'),
    'features/notes/frontend/feature.ts',
  );
  assert.doesNotMatch(paged, manifestIcon);
  for (const shape of [
    ['--page', '--mandatory'],
    ['--tab-of', 'uniforms'],
  ])
    assert.match(
      file(await feature('notes', '--settings', ...shape), 'features/notes/frontend/feature.ts'),
      manifestIcon,
    );
  await refused(/--icon names a lucide-react icon/, 'notes', '--settings', '--icon', 'Truckk');
  // Its manifest lists the section, for those holding the manage permission it declares.
  const manifest = file(plan, 'features/notes/feature.rs');
  assert.match(manifest, /^mod backend;$/m);
  assert.match(
    manifest,
    /perm\("notes\.manage", "Manage Notes", \d+\)\.implies\(&\["notes\.view"\]\)/,
  );
  assert.match(manifest, /settings: &\[backend::settings::GENERAL\],/);
  assert.match(file(plan, 'features/notes/backend/mod.rs'), /^pub\(crate\) mod settings;$/m);
  const settings = file(plan, 'features/notes/backend/settings.rs');
  assert.match(settings, /section\(\s*"notes\.general",\s*"General",\s*"notes\.manage",/);
  // Its backend reads it typed, in one keyed lookup, which its test reads back from a save.
  assert.match(settings, /dispatch_core::settings::read\(db, dsp, &GENERAL\)/);
  assert.match(file(plan, 'features/notes/tests/backend/settings.rs'), /settings::save\(/);
  assert.match(file(plan, 'features/notes/Cargo.toml'), /^serde = \{ workspace = true \}$/m);
  assert.match(file(plan, 'features/notes/README.md'), /^- \*\*Settings:\*\* `notes\.general`/m);
  assert.match(plan.notes.join('\n'), /`\?tab=notes`/);
  // Mandatory, it has no frontend, and its settings' test is its backend's: no manifest test.
  assert.deepEqual(writes(await feature('lobby', '--mandatory', '--settings'), 'features/lobby'), [
    'Cargo.toml',
    'README.md',
    'backend/mod.rs',
    'backend/settings.rs',
    'feature.rs',
    'tests/backend/settings.rs',
  ]);
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
  assert.match(manifest, /switch: optional\(/);
  assert.match(file(plan, 'app/backend/features.rs'), /&dispatch_front::FEATURE,/);
  await refused(/add --page, --sidebar or --tab-of/, 'front', '--no-backend');
  await refused(/add --page, --sidebar or --tab-of/, 'front', '--settings', '--no-backend');
  for (const piece of [['--api'], ['--settings'], ['--tables', 'dsp'], ['--keeps', 'cortex.dvic']])
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

test('--sidebar writes a sidebar of pages, each a page under it, and refuses ids taken', async () => {
  const rota = await feature('rota', '--sidebar', 'shifts,swaps', '--api');
  assert.deepEqual(
    writes(rota, 'features/rota').filter((name) => /frontend|browser/.test(name)),
    [
      'frontend/ShiftsPage.tsx',
      'frontend/SwapsPage.tsx',
      'frontend/feature.ts',
      'frontend/index.ts',
      'frontend/platform-slots.ts',
      'tests/browser/rota.spec.ts',
    ],
  );
  const manifest = file(rota, 'features/rota/frontend/feature.ts');
  assert.match(
    manifest,
    /sidebars: \[\{ id: 'rota', scope: 'dsp', label: 'Rota', [^\n]*\n\s*routes: \[/,
  );
  assert.equal(manifest.match(/parent: 'rota',/g)?.length, 2);
  // The first page shows the summary, warmed as it opens, as a page of its own does.
  assert.equal(manifest.match(/warm\(\[rotaSummaryUrl\]\)/g)?.length, 1);
  assert.match(file(rota, 'features/rota/frontend/ShiftsPage.tsx'), /useRotaSummary\(\)/);
  assert.doesNotMatch(file(rota, 'features/rota/frontend/SwapsPage.tsx'), /Summary/);
  assert.match(
    file(rota, 'features/rota/tests/browser/rota.spec.ts'),
    /getByRole\('navigation', \{ name: 'Rota', exact: true \}\)/,
  );
  assert.match(rota.notes.join('\n'), /--page shifts` shows the first page of its sidebar/);

  await refused(/--sidebar and --page/, 'rota', '--sidebar', 'shifts', '--page');
  await refused(/--sidebar and --tab-of/, 'rota', '--sidebar', 'shifts', '--tab-of', 'timecard');
  await refused(/lowercase letters, digits and hyphens/, 'rota', '--sidebar', 'Shifts');
  await refused(/names shifts twice/, 'rota', '--sidebar', 'shifts,shifts');
  await refused(/dvic is another page's id/, 'rota', '--sidebar', 'dvic');
  // Its sidebar takes the feature's id, which Home's page has.
  await refused(/overview is a page's id already/, 'overview', '--sidebar', 'shifts');
});

test('a preset stands for its options, and --tabs, --connection and --people own write theirs', async () => {
  // An option given beside a preset wins over the preset's own.
  const named = await feature('desk', '--preset', 'page-with-tabs', '--tabs', 'today,week');
  assert.match(
    file(named, 'features/desk/frontend/DeskPage.tsx'),
    /\['today', 'Today'\],\s*\['week', 'Week'\]/,
  );
  assert.throws(() => withPreset(['desk', '--preset', 'keeper']), /add --keeps/);
  assert.throws(() => withPreset(['desk', '--preset', 'kiosk']), /--preset names one of/);

  const tabs = await feature('desk', '--tabs', 'overview,history');
  assert.match(file(tabs, 'features/desk/frontend/feature.ts'), /nav: true,\n\s+tabs: true,/);
  assert.match(file(tabs, 'features/desk/frontend/DeskPage.tsx'), /usePageTabs\('desk', \[/);
  await refused(/--tabs names each tab/, 'desk', '--tabs', 'Overview');

  const connection = await feature('desk', '--connection', '--api');
  assert(connection.files.has('features/desk/frontend/connection/DeskCard.tsx'));
  assert.match(file(connection, 'features/desk/feature.rs'), /dsp_connection: true,/);
  assert.match(file(connection, 'features/desk/frontend/feature.ts'), /reads: \[deskSummaryUrl\],/);

  const source = await feature('desk', '--tables', 'dsp', '--people', 'own');
  const people = file(source, 'features/desk/backend/people.rs');
  assert.match(people, /pub const SOURCE: Source = Source::new\(&SourceInfo \{\s*id: "desk",/);
  assert.match(people, /^\s+SOURCE,$/m);
  assert.doesNotMatch(file(source, 'features/desk/Cargo.toml'), /\.\.\/\.\.\/collectors\//);
});

test("listing a feature keeps on the features the MCP's tools need", async () => {
  const links = (text: string) => text.match(/"dispatch-mcp\/\w+"/g) ?? [];
  const before = links(fs.readFileSync(path.join(root, 'app/backend/Cargo.toml'), 'utf8'));
  assert(before.length, 'no tool needs a feature');
  assert.deepEqual(
    links(file(await feature('parking', '--page'), 'app/backend/Cargo.toml')),
    before,
  );
});
