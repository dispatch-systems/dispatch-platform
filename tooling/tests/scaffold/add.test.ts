import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { mergeTypeScript, planAdd } from '../../scaffold/add.js';
import { planFeature } from '../../scaffold/new-feature.js';
import { changed, copyWorkspace, feature, file, refused, root, writes } from './support.js';

// add: a piece merged into a feature that exists, and the contracts features expose.

// A copy of the repository's workspace, which these tests write into.
let copy = '';
before(() => {
  copy = copyWorkspace();
});
after(() => fs.rmSync(copy, { recursive: true, force: true }));

const add = async (...argv: string[]) => (await planAdd(root, argv)).plan;

test('add sidebar gives a feature a sidebar its page moves under, and refuses one it has', async () => {
  const dvic = await add('sidebar', 'drafts,archive', '--to', 'dvic');
  const manifest = changed(dvic, 'features/dvic/frontend/feature.ts');
  // The sidebar where the piece has it, before the pages, and the page DVIC had under it, after
  // its name and icon.
  assert.match(manifest, /sidebars: \[[^\]]*\],\n\s*routes: \[/);
  assert.match(manifest, /label: 'DVIC',\n\s*icon: ClipboardCheck,\n\s*parent: 'dvic',/);
  assert.equal(manifest.match(/parent: 'dvic',/g)?.length, 3);
  assert.match(dvic.notes.join('\n'), /Its page DVIC is under its sidebar now/);
  await assert.rejects(
    planAdd(root, ['sidebar', 'drafts', '--to', 'settings']),
    /has a sidebar of pages already/,
  );
});

test('add merges a piece into a feature that exists: its slot entries, fields and permissions', async () => {
  // Settings on Uniforms, which holds uniforms.manage already: its section joins the backend
  // and the manifest it has, and nothing of its frontend.
  const settings = await add('settings', '--to', 'uniforms');
  assert.deepEqual(writes(settings, 'features/uniforms'), [
    'backend/settings.rs',
    'tests/backend/settings.rs',
  ]);
  assert(![...settings.changes.keys()].some((name) => name.includes('/frontend/')));
  const manifest = changed(settings, 'features/uniforms/feature.rs');
  assert.match(manifest, /settings: &\[backend::settings::GENERAL\],/);
  assert.equal(
    manifest.match(/perm\("uniforms\.manage"/g)?.length,
    1,
    'it adds no permission it has',
  );
  // Its build lists what the section and its test need already.
  assert(!settings.changes.has('features/uniforms/Cargo.toml'));
  assert.match(
    changed(settings, 'features/uniforms/backend/mod.rs'),
    /^pub\(crate\) mod settings;$/m,
  );

  // A tab of another's page, on Documents, whose comments hold apostrophes: one platformSlots,
  // and no permission, since a tab needs none.
  const tab = await add('tab-of', 'uniforms', '--to', 'documents');
  const documents = changed(tab, 'features/documents/frontend/feature.ts');
  assert.equal(documents.match(/^\s+platformSlots:/gm)?.length, 1);
  assert.equal(documents.match(/^import .* from 'react';$/gm)?.length, 1);
  assert.match(documents, /pageTabs: \[\s*\{\s*page: 'uniforms',/);
  assert(!tab.changes.has('features/documents/feature.rs'));

  // An API on Home: its routes, its permission, its types' export, and the ts Cargo feature.
  const api = await add('api', '--to', 'home');
  const home = changed(api, 'features/home/feature.rs');
  assert.match(home, /^mod api;$/m);
  assert.match(home, /routes: api::routes::routes,/);
  assert.match(home, /perm\("home\.view", /);
  assert.match(home, /pub fn typescript\(/);
  assert.match(
    changed(api, 'features/home/Cargo.toml'),
    /\[features\]\nts = \["dep:ts-rs", "dispatch-core\/ts"\]/,
  );

  // People of a table it has, by a collector's IDs: declared, with nothing of a new table's.
  const people = await add('people', 'paycom', '--to', 'uniforms', '--table', 'uniform_events');
  const uniforms = changed(people, 'features/uniforms/feature.rs');
  assert.match(uniforms, /depends_on: &\["paycom"\],/);
  assert.match(uniforms, /people: &\[&backend::people::PEOPLE\],/);
  assert.doesNotMatch(
    uniforms,
    /UniformsStore|own_migrations: &\[OwnMigrations \{\s*kind: Kind::DSP,\s*list: &\[Migration \{\s*id: 1,/,
  );
  assert(![...people.files.keys()].some((file) => /storage|migrations/.test(file)));
  // Its tests install it with the collector it now depends on, as the registry wants.
  const installs = [...people.changes].filter(([file]) => file.includes('/tests/'));
  assert(installs.length, 'no test of uniforms installs it');
  for (const [, text] of installs)
    assert.match(text, /testing::install\(&\[[^\]]*&dispatch_paycom::COLLECTOR\]/);

  // A manage permission implies only what the feature has: Team has no team.view. Nor has it
  // a backend/, which its settings bring.
  const teamSettings = await add('settings', '--to', 'team');
  const team = changed(teamSettings, 'features/team/feature.rs');
  // Its page is no switch's, so its manifest gives its page of Settings the page's icon.
  assert.match(changed(teamSettings, 'features/team/frontend/feature.ts'), /^ {2}icon: Users,$/m);
  assert.match(team, /perm\("team\.manage", "Manage Team & Roles", \d+\),/);
  assert.match(team, /^mod backend;$/m);
  // A name a nested use tree imports, as Documents' Sql, is not imported again; a module a
  // feature declares without pub(crate), as Timecard's storage, is not declared again; and a
  // piece without a screen writes no browser test.
  const drive = await add('people', 'amazon', '--to', 'documents', '--table', 'documents_files');
  assert.equal(changed(drive, 'features/documents/feature.rs').match(/\bSql\b[,}]/g)?.length, 1);
  const punches = await add('people', 'paycom', '--to', 'timecard', '--table', 'employees');
  assert.doesNotMatch(
    changed(punches, 'features/timecard/backend/mod.rs'),
    /pub\(crate\) mod storage;/,
  );
  assert(![...punches.files.keys()].some((file) => file.includes('/tests/browser/')));
  // Its people read the table where the feature keeps it: Timecard's employees in Paycom's.
  assert.match(
    file(punches, 'features/timecard/backend/people.rs'),
    /store\.collector\(dsp, dispatch_paycom::PROVIDER\)/,
  );
  // A constant the manifest has under the same name takes the feature's: a loadTab of its own
  // stays, and the tab's loads as loadNotesTab.
  const merged = mergeTypeScript(
    "const loadTab = () => import('./mine.js');\n\nexport const feature: FrontendFeature = {\n  name: 'notes',\n};\n",
    "const loadTab = () => import('./NotesTab.js');\n\nexport const feature: FrontendFeature = {\n  name: 'notes',\n  pageTabs: [{ id: 'notes', load: loadTab }],\n};\n",
    'Notes',
  );
  assert.match(merged, /^const loadTab = \(\) => import\('\.\/mine\.js'\);$/m);
  assert.match(merged, /^const loadNotesTab = \(\) => import\('\.\/NotesTab\.js'\);$/m);
  assert.match(merged, /load: loadNotesTab \}/);
  // A README's line goes after its last line of what the feature declares, however long.
  assert.match(
    changed(settings, 'features/uniforms/README.md'),
    /deletes none of them\.\n- \*\*Settings:\*\*/,
  );
  // A permission named in another's retired list is not the feature's retired one.
  assert(
    !(await add('page', '--to', 'weekly_scorecard')).changes.has(
      'features/weekly_scorecard/feature.rs',
    ),
  );
  // An added permission's order is one no other feature holds.
  const profiles = changed(
    await add('page', '--to', 'driver_profiles'),
    'features/driver_profiles/feature.rs',
  );
  const order = /perm\("driver_profiles\.view", "[^"]+", (\d+)\)/.exec(profiles)![1];
  const others = fs
    .readdirSync(path.join(root, 'features'))
    .filter(
      (name) =>
        name !== 'driver_profiles' &&
        fs.existsSync(path.join(root, 'features', name, 'feature.rs')),
    )
    .map((name) => fs.readFileSync(path.join(root, 'features', name, 'feature.rs'), 'utf8'));
  assert(
    !others.some((text) => new RegExp(`perm\\("[^"]+", "[^"]+", ${order}\\)`).test(text)),
    `${order} is taken`,
  );
  // A README's line joins its list, whatever its last item's bold lead says.
  assert.match(
    changed(await add('api', '--to', 'home'), 'features/home/README.md'),
    /\*\*[^\n]*\n- \*\*API:\*\*/,
  );
  // A permissions field it lacks holds only what the piece uses.
  assert.doesNotMatch(
    changed(await add('settings', '--to', 'home'), 'features/home/feature.rs'),
    /home\.view/,
  );

  // A manifest alone's stamped test goes once a piece makes it more, with its module, which
  // was the manifest's last item.
  const bare = await planFeature(copy, ['bare_demo', '--mandatory']);
  for (const [name, content] of bare.plan.files) {
    fs.mkdirSync(path.dirname(path.join(copy, name)), { recursive: true });
    fs.writeFileSync(path.join(copy, name), content);
  }
  const grown = (await planAdd(copy, ['api', '--to', 'bare_demo'])).plan;
  assert.deepEqual(grown.removed, [
    'features/bare_demo/tests/backend/feature.rs',
    'features/bare_demo/backend/mod.rs',
  ]);
  assert.doesNotMatch(changed(grown, 'features/bare_demo/feature.rs'), /mod (tests|backend);/);
  // What the author wrote into either stays: their function, and their test without the
  // stamped assertion.
  const written = await planFeature(copy, ['bare_kept', '--mandatory']);
  for (const [name, content] of written.plan.files) {
    const own = name.endsWith('/backend/mod.rs')
      ? `${content}pub(crate) fn greeting() {}\n`
      : name.endsWith('/tests/backend/feature.rs')
        ? `${content}\n#[test]\nfn greets() {}\n`
        : content;
    fs.mkdirSync(path.dirname(path.join(copy, name)), { recursive: true });
    fs.writeFileSync(path.join(copy, name), own);
  }
  const kept = (await planAdd(copy, ['api', '--to', 'bare_kept'])).plan;
  assert.equal(kept.removed, undefined);
  assert.match(changed(kept, 'features/bare_kept/feature.rs'), /^mod backend;$/m);
  const stamped = changed(kept, 'features/bare_kept/tests/backend/feature.rs');
  assert.match(stamped, /fn greets\(\)/);
  assert.doesNotMatch(stamped, /permissions\.is_empty/);

  await assert.rejects(add('api', '--to', 'uniforms'), /uniforms has api\/ already/);
  await assert.rejects(add('settings', '--to', 'documents'), /documents retired documents\.manage/);
  await assert.rejects(add('page', '--to', 'documents'), /documents has a page already/);
  await assert.rejects(add('people', 'paycom', '--to', 'uniforms'), /add --table/);
  await assert.rejects(
    add('people', 'paycom', '--to', 'uniforms', '--table', 'nowhere'),
    /--table names one of uniforms's tables/,
  );
  await assert.rejects(
    add('people', 'paycom', '--to', 'dvic', '--table', 'dvic_runs'),
    /in dvic's own database/,
  );
  await assert.rejects(
    add('settings', '--to', 'routes'),
    /routes declares its settings in backend/,
  );
  await assert.rejects(add('tab-of', 'dvic', '--to', 'dvic'), /own tabs/);
  await assert.rejects(add('wings', '--to', 'uniforms'), /Name one piece/);
  await assert.rejects(add('settings', '--to', 'nowhere'), /--to names a feature/);
});

test('--contract writes a contract crate on core alone, what fulfils it, its exposure and their test', async () => {
  const plan = await feature('parking', '--contract', 'parking_spots');
  assert.deepEqual(
    writes(plan, 'features/parking').filter((name) =>
      /^(contract\/|backend\/exposed|tests\/backend\/exposed)/.test(name),
    ),
    [
      'backend/exposed.rs',
      'contract/Cargo.toml',
      'contract/contract.rs',
      'tests/backend/exposed.rs',
    ],
  );
  const crate = file(plan, 'features/parking/contract/Cargo.toml');
  assert.match(crate, /^name = "dispatch-parking-contract"$/m);
  assert.match(crate, /^path = "contract\.rs"$/m);
  assert.deepEqual(crate.split('[dependencies]')[1]!.trim().split('\n'), [
    'dispatch-core = { path = "../../../core" }',
  ]);
  const contract = file(plan, 'features/parking/contract/contract.rs');
  assert.match(contract, /^pub trait ParkingSpots: Send \+ Sync \{$/m);
  assert.match(contract, /exposes::for_dsp::<dyn ParkingSpots>\(db, dsp\)/);
  assert.match(
    file(plan, 'features/parking/backend/exposed.rs'),
    /^impl ParkingSpots for Exposed \{$/m,
  );
  const manifest = file(plan, 'features/parking/feature.rs');
  assert.match(
    manifest,
    /dyn dispatch_parking_contract::ParkingSpots,\n\s+&backend::exposed::EXPOSED,\n\s+"parking_spots",/,
  );
  assert.match(
    file(plan, 'features/parking/Cargo.toml'),
    /^dispatch-parking-contract = \{ path = "contract" \}$/m,
  );
  const test = file(plan, 'features/parking/tests/backend/exposed.rs');
  assert.match(test, /fn parking_spots_is_offered_only_while_parking_is_on\(\)/);
  assert.match(test, /db\.set_feature\(&dsp, "parking", false, &owner\)/);
  // A mandatory feature's is offered to every DSP, with no switch to turn off.
  const always = file(
    await feature('lot', '--mandatory', '--contract', 'lot_spaces'),
    'features/lot/tests/backend/exposed.rs',
  );
  assert.match(always, /fn lot_spaces_is_offered_to_every_dsp\(\)/);
  assert.doesNotMatch(always, /set_feature/);
  await refused(/documents exposes files already/, 'parking', '--contract', 'files');
  await refused(
    /--contract names what it offers in lowercase words/,
    'parking',
    '--contract',
    'Spots',
  );
  await refused(
    /--no-backend leaves out the Rust logic that --contract needs/,
    'parking',
    '--no-backend',
    '--page',
    '--contract',
    'spots',
  );

  // Added to a feature that has none, with what its tests install: Routes and Daily
  // Performance, in the copy, as they were before their own contracts.
  const uncontracted = (name: string) => {
    const dir = path.join(copy, 'features', name);
    for (const entry of ['contract', 'backend/exposed.rs', 'tests/backend/exposed.rs'])
      fs.rmSync(path.join(dir, entry), { recursive: true, force: true });
    const strip = (file: string, pattern: RegExp) => {
      const content = fs.readFileSync(path.join(dir, file), 'utf8');
      assert.match(content, pattern, `features/${name}/${file} has its contract's lines`);
      fs.writeFileSync(path.join(dir, file), content.replace(pattern, ''));
    };
    strip('Cargo.toml', /^dispatch-[\w-]+-contract = \{ path = "contract" \}\n/m);
    strip('backend/mod.rs', /^pub\(crate\) mod exposed;\n/m);
    strip(
      'feature.rs',
      /\n\s*\/\/ What it offers[^\n]*\n\s*exposes: &\[\s*dispatch_core::expose!\([\s\S]*?\n\s*\),?\s*\],/,
    );
    strip('README.md', /\n+- \*\*Exposed:\*\*[\s\S]*?(?=\n\n|\n- |\n#|\n?$)/);
  };
  uncontracted('routes');
  uncontracted('daily_performance');
  const addToCopy = async (...argv: string[]) => (await planAdd(copy, argv)).plan;
  const added = await addToCopy('contract', 'route_counts', '--to', 'routes');
  assert(added.files.has('features/routes/contract/contract.rs'));
  assert.match(
    file(added, 'features/routes/tests/backend/exposed.rs'),
    /testing::install\(&\[&dispatch_cortex::COLLECTOR\], &\[&crate::FEATURE\]\)/,
  );
  assert.match(
    changed(added, 'features/routes/feature.rs'),
    /exposes: &\[dispatch_core::expose!\(/,
  );
  assert.match(changed(added, 'features/routes/backend/mod.rs'), /^pub\(crate\) mod exposed;$/m);
  assert.match(changed(added, 'features/routes/README.md'), /^- \*\*Exposed:\*\* `route_counts`/m);
  await assert.rejects(
    planAdd(root, ['contract', 'more', '--to', 'dvic']),
    /dvic has contract\/ already/,
  );
  // Daily Performance's tests name Cortex by an alias, `cortex::COLLECTOR`, and its test
  // installs it all the same.
  assert.match(
    file(
      await addToCopy('contract', 'daily_counts', '--to', 'daily_performance'),
      'features/daily_performance/tests/backend/exposed.rs',
    ),
    /testing::install\(&\[&dispatch_cortex::COLLECTOR\], &\[&crate::FEATURE\]\)/,
  );
  for (const name of ['routes', 'daily_performance'])
    fs.cpSync(path.join('features', name), path.join(copy, 'features', name), { recursive: true });
  // Driver Match exposes the directory already, and is mandatory: a second entry joins it,
  // offered to every DSP.
  const second = await add('contract', 'match_counts', '--to', 'driver_match');
  const driverMatch = changed(second, 'features/driver_match/feature.rs');
  assert.match(driverMatch, /"directory",[\s\S]*"match_counts",/);
  assert.equal(driverMatch.match(/^\s+exposes: &\[/gm)?.length, 1);
  assert.doesNotMatch(
    changed(second, 'features/driver_match/README.md'),
    /`match_counts`.*while it's on/,
  );
  // Home has no backend/: it gains one, and core's test helpers for its test.
  const home = await add('contract', 'home_counts', '--to', 'home');
  assert(home.files.has('features/home/backend/mod.rs'));
  assert.match(changed(home, 'features/home/feature.rs'), /^mod backend;$/m);
  assert.match(
    changed(home, 'features/home/Cargo.toml'),
    /\[dev-dependencies\]\ndispatch-core = \{ path = "..\/..\/core", features = \["testing"\] \}/,
  );
  // A name a manifest exposes, not yet generated into the feature map, is taken too.
  const homeManifest = path.join(copy, 'features/home/feature.rs');
  const original = fs.readFileSync(homeManifest, 'utf8');
  fs.writeFileSync(
    homeManifest,
    original.replace(
      /\.\.feature\("home"\)/,
      'exposes: &[dispatch_core::expose!(dyn X, &Y, "home_spots", "Its spots.")],\n    ..feature("home")',
    ),
  );
  try {
    await assert.rejects(
      planFeature(copy, ['parking', '--contract', 'home_spots']),
      /home exposes home_spots already/,
    );
  } finally {
    fs.writeFileSync(homeManifest, original);
  }
});

test('--shown writes a contract a page shows, by its name and permission, and the name pages use', async () => {
  const plan = await feature('parking', '--api', '--contract', 'parking_spots', '--shown');
  assert.match(
    file(plan, 'features/parking/contract/contract.rs'),
    /^pub trait ParkingSpots: Shown \+ Send \+ Sync \{$/m,
  );
  assert.match(
    file(plan, 'features/parking/backend/exposed.rs'),
    /^impl Shown for Exposed \{\n\s+fn name\(&self\) -> &'static str \{\n\s+"Parking"\n[\s\S]*?"parking\.view"/m,
  );
  assert.match(file(plan, 'features/parking/feature.rs'), /"parking_spots",\n[^\n]*\n\s+shown,\n/);
  assert.match(
    changed(plan, 'core/shell/frontend/runtime/exposes.ts'),
    /^export type ExposedName = 'files' \| 'parking_spots';$/m,
  );
  // Added to a feature that exists, shown to those who view it.
  const added = await add('contract', 'sizes', '--to', 'uniforms', '--shown');
  assert.match(changed(added, 'features/uniforms/feature.rs'), /"sizes",\n[^\n]*\n\s+shown,\n/);
  assert.match(file(added, 'features/uniforms/backend/exposed.rs'), /"uniforms\.view"/);
  assert.match(changed(added, 'core/shell/frontend/runtime/exposes.ts'), /'files' \| 'sizes'/);
  await refused(/--shown is what a page shows of what it exposes/, 'parking', '--shown');
  await refused(
    /--shown needs the permission that shows what it keeps/,
    'lot',
    '--mandatory',
    '--contract',
    'lot_spaces',
    '--shown',
  );
  await assert.rejects(
    planAdd(root, ['contract', 'counts', '--to', 'driver_match', '--shown']),
    /driver_match\.view, which driver_match lacks/,
  );
  await assert.rejects(
    planAdd(root, ['api', '--to', 'home', '--shown']),
    /--shown is what a page shows of a contract/,
  );
});
