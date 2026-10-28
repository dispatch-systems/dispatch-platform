import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import * as prettier from 'prettier';
import { planCollector } from '../../scaffold/new-collector.js';
import { planIntegration } from '../../scaffold/new-integration.js';
import { planSlot } from '../../scaffold/new-slot.js';
import { planTool } from '../../scaffold/new-tool.js';
import { crates, tools, wiredTools } from '../../scaffold/wire.js';
import { changed, feature, file, registry, root, temporary, writes } from './support.js';

// The generators besides new feature and add: tools, slots, collectors and integrations, and
// how everything they write is formatted.

const collector = async (...argv: string[]) => (await planCollector(root, argv)).plan;
const tool = async (...argv: string[]) => (await planTool(root, argv)).plan;
const slot = async (...argv: string[]) => (await planSlot(root, argv)).plan;
/** A workspace of what a tool's plan reads: the features and the MCP's tools. */
const toolWorkspace = () => {
  const workspace = temporary();
  for (const entry of ['features', 'mcp/tools'])
    fs.cpSync(entry, path.join(workspace, entry), { recursive: true });
  return workspace;
};

test('a tool is a file and a test in mcp/tools, that works as written', async () => {
  const plan = await tool(
    'short_inspections',
    '--features',
    'dvic',
    '--description',
    'List short ones.',
  );
  assert.deepEqual(writes(plan, 'mcp'), [
    'tests/backend/tools/short_inspections.rs',
    'tools/short_inspections.rs',
  ]);
  const written = file(plan, 'mcp/tools/short_inspections.rs');
  assert.match(written, /const NAME: &'static str = "short_inspections";/);
  assert.match(written, /const DESCRIPTION: &'static str = "List short ones\.";/);
  assert.match(written, /const FEATURES: &'static \[&'static str\] = &\["dvic"\];/);
  // It reads, about one DSP, unless it says otherwise, and never says who may use it.
  assert.doesNotMatch(written, /EFFECT|SCOPE|ToolLevel|allow/);
  assert.match(written, /#\[path = "\.\.\/tests\/backend\/tools\/short_inspections\.rs"\]/);
  // The wiring lists it, so the scaffold changes nothing else.
  assert.deepEqual([...plan.changes.keys()], []);
  // Its test switches on what it needs, and grants it as an owner would.
  const test = file(plan, 'mcp/tests/backend/tools/short_inspections.rs');
  assert.match(test, /db\.set_feature\(&dsp, "dvic", true, &owner\)/);
  assert.match(test, /agent\(&db, &\[&dsp\], &\[\("short_inspections", ToolLevel::Read\)\]\)/);
  assert.match(test, /"switched_off"/);

  // One of several actions, some that change something, at a part of a feature.
  const review = await tool(
    'review_meal_breaks',
    '--features',
    'timecard.meal_breaks',
    '--actions',
    'list,approve',
    '--changing',
    'approve',
  );
  const reviewed = file(review, 'mcp/tools/review_meal_breaks.rs');
  assert.match(reviewed, /const EFFECT: Effect = Effect::Changes;/);
  assert.match(
    reviewed,
    /#\[serde\(tag = "action", rename_all = "snake_case", deny_unknown_fields\)\]/,
  );
  assert.match(reviewed, /^ {4}List \{\},$/m);
  assert.match(reviewed, /ReviewMealBreaksAction::Approve \{\} => Effect::Changes,/);
  assert.match(reviewed, /cx\.write\(/);
  const reviewTest = file(review, 'mcp/tests/backend/tools/review_meal_breaks.rs');
  assert.match(reviewTest, /"timecard", true[\s\S]*"timecard\.meal_breaks", true/);
  assert.match(reviewTest, /ToolLevel::Change/);
  assert.match(reviewTest, /json!\(\{"action": "approve"\}\)/);

  // About several DSPs, or the connection itself.
  const survey = file(await tool('survey', '--scope', 'dsps', '--changes'), 'mcp/tools/survey.rs');
  assert.match(survey, /const SCOPE: Scope = Scope::Dsps;/);
  assert.match(survey, /pub dsps: Vec<String>,/);
  const own = await tool('my_connection', '--scope', 'connection');
  assert.match(file(own, 'mcp/tools/my_connection.rs'), /const SCOPE: Scope = Scope::Connection;/);
  assert.match(
    file(own, 'mcp/tests/backend/tools/my_connection.rs'),
    /let caller = agent\(&db, &\[&dsp\], &\[\]\);/,
  );
});

test('--reads writes a tool that needs a feature and reads what it exposes, never its storage', async () => {
  const plan = await tool('dvic_days', '--reads', 'dvic');
  const written = file(plan, 'mcp/tools/dvic_days.rs');
  assert.match(written, /^use dispatch_dvic_contract as dvic;$/m);
  assert.match(written, /const FEATURES: &'static \[&'static str\] = &\["dvic"\];/);
  assert.match(written, /let Some\(_dvic\) = dvic::for_dsp\(db, &at\)\? else \{/);
  assert.match(written, /"DVIC is switched off at this DSP\. Tell the user\."/);
  assert.doesNotMatch(written, /DvicStore|dispatch_dvic::/);
  // Named among its features already, it needs it once.
  const both = file(
    await tool('dvic_weeks', '--features', 'dvic', '--reads', 'dvic'),
    'mcp/tools/dvic_weeks.rs',
  );
  assert.match(both, /const FEATURES: &'static \[&'static str\] = &\["dvic"\];/);
  const refusedTool = async (pattern: RegExp, ...argv: string[]) =>
    assert.rejects(planTool(root, argv), pattern);
  await refusedTool(/--reads names a feature, and teleport is none/, 'beam', '--reads', 'teleport');
  await refusedTool(
    /home exposes no contract: `dispatchdev add contract/,
    'beam',
    '--reads',
    'home',
  );
  await refusedTool(/--reads reads for one DSP/, 'beam', '--reads', 'dvic', '--scope', 'dsps');
  // Driver Match is mandatory, on for every DSP: a tool needing it never switches it in its
  // test, while it switches the optional feature whose contract it reads.
  const always = await tool(
    'directory_reader',
    '--reads',
    'documents',
    '--features',
    'driver_match',
  );
  const test = file(always, 'mcp/tests/backend/tools/directory_reader.rs');
  assert.doesNotMatch(test, /"driver_match", (true|false)/);
  assert.match(test, /db\.set_feature\(&dsp, "documents", true, &owner\)/);
  // A part a manifest marks mandatory isn't switched either, while its feature is.
  const workspace = toolWorkspace();
  try {
    const dvic = path.join(workspace, 'features/dvic/feature.rs');
    fs.writeFileSync(
      dvic,
      fs
        .readFileSync(dvic, 'utf8')
        .replace('tab("dvic.day", "Day")', 'tab("dvic.day", "Day").mandatory()'),
    );
    const part = file(
      (await planTool(workspace, ['day_reader', '--features', 'dvic.day'])).plan,
      'mcp/tests/backend/tools/day_reader.rs',
    );
    assert.match(part, /db\.set_feature\(&dsp, "dvic", true, &owner\)/);
    assert.doesNotMatch(part, /"dvic\.day", (true|false)/);
  } finally {
    fs.rmSync(workspace, { recursive: true, force: true });
  }
});

test('a tool written into mcp/tools is listed by the wiring, and its name is then taken', async () => {
  const workspace = toolWorkspace();
  try {
    const first = (await planTool(workspace, ['short_inspections', '--features', 'dvic'])).plan;
    for (const [name, content] of [...first.files, ...first.changes]) {
      fs.mkdirSync(path.dirname(path.join(workspace, name)), { recursive: true });
      fs.writeFileSync(path.join(workspace, name), content);
    }
    const listed = wiredTools(tools(workspace, crates(workspace)));
    assert.match(listed, /^#\[cfg\(feature = "dvic"\)\]\nmod short_inspections;$/m);
    assert.match(listed, /^ {4}&short_inspections::ShortInspections,$/m);
    await assert.rejects(planTool(workspace, ['short_inspections']), /is taken/);
  } finally {
    fs.rmSync(workspace, { recursive: true, force: true });
  }
});

test('a tool has a name of its own, and needs features and actions there are', async () => {
  const refusedTool = async (pattern: RegExp, ...argv: string[]) =>
    assert.rejects(planTool(root, argv), pattern);
  await refusedTool(/not a tool's name/, 'Short-Inspections');
  await refusedTool(/whoami is taken: mcp\/tools\/whoami\.rs declares it/, 'whoami');
  await refusedTool(/teleport is no feature or part of one/, 'beam', '--features', 'teleport');
  await refusedTool(/--scope is dsp, dsps, connection/, 'beam', '--scope', 'galaxy');
  await refusedTool(/--changing names actions/, 'beam', '--changing', 'approve');
  await refusedTool(
    /reject is none of its --actions/,
    'beam',
    '--actions',
    'list',
    '--changing',
    'reject',
  );
  await refusedTool(/With --actions, name the actions/, 'beam', '--actions', 'list', '--changes');
  await refusedTool(/only reads/, 'beam', '--scope', 'connection', '--changes');
  await refusedTool(/only reads/, 'beam', '--scope', 'connection', '--features', 'dvic');
  await refusedTool(/ending with a full stop/, 'beam', '--description', 'List');
  await refusedTool(/120 characters at most/, 'beam', '--description', 'List. '.repeat(21));
  await refusedTool(/Name the tool, and only the tool/);
  await refusedTool(/Name the tool, and only the tool/, 'dvic', 'beam');
});

test('a slot is a backend module and a frontend module, declared, checked, re-exported and catalogued', async () => {
  const plan = await slot('badges', '--description', 'Its badges on the DSPs page.');
  assert.deepEqual(writes(plan, 'core'), [
    'manifest/backend/badges.rs',
    'shell/frontend/runtime/slots/badges.ts',
  ]);
  const backend = file(plan, 'core/manifest/backend/badges.rs');
  assert.match(backend, /pub struct Badge \{/);
  assert.match(backend, /pub const fn badge\(id: &'static str, label: &'static str\) -> Badge/);
  assert.match(backend, /pub fn badges\(&self\)/);
  assert.match(backend, /pub\(crate\) fn check\(registry: &Registry\)/);
  // The manifest gains the slot in each of its four places, each among its kind in order.
  const manifest = file(plan, 'core/manifest/backend/mod.rs');
  assert.match(manifest, /^pub mod audit;\npub mod badges;\npub mod caching;$/m);
  assert.match(manifest, /^pub use audit::Audit;\npub use badges::\{Badge, badge\};$/m);
  assert.match(
    manifest,
    / {4}\/\/\/ Its badges on the DSPs page\.\n {4}pub badges: &'static \[Badge\],\n\}/,
  );
  assert.match(manifest, / {8}on: &\[\],\n {8}badges: &\[\],\n {4}\}/);
  assert.match(manifest, / {8}badges::check\(self\);\n {8}agents::check\(self\);/);
  // The frontend slot adds its field by declaration merging, and the facade re-exports it.
  const frontend = file(plan, 'core/shell/frontend/runtime/slots/badges.ts');
  assert.match(
    frontend,
    /declare module '\.\/installed\.js' \{\n {2}interface FrontendFeature \{\n {4}\/\*\* Its badges\. \*\/\n {4}badges\?: readonly Badge\[\];/,
  );
  assert.match(frontend, /export const badges = \(view: DspView\) =>/);
  const facade = file(plan, 'core/shell/frontend/runtime/slots.ts');
  assert.match(
    facade,
    /export type \{ Badge \} from '\.\/slots\/badges\.js';\nexport \{ badges \} from '\.\/slots\/badges\.js';\n$/,
  );
  // Both tables of the catalogue gain its row, so the catalogue rule passes as written.
  const catalogue = file(plan, 'core/manifest/README.md');
  assert.equal(
    catalogue.match(/^\| `badges` +\| `badges` +\| Its badges on the DSPs page\. +\|$/gm)?.length,
    2,
  );
  // One side alone, and one of several named for itself.
  const only = await slot('shortcuts', '--frontend-only');
  assert.deepEqual(writes(only, 'core'), ['shell/frontend/runtime/slots/shortcuts.ts']);
  assert.deepEqual([...only.changes.keys()].sort(), [
    'core/manifest/README.md',
    'core/shell/frontend/runtime/slots.ts',
  ]);
  const own = await slot('crews', '--each', 'crew', '--backend-only');
  assert.match(file(own, 'core/manifest/backend/crews.rs'), /pub struct Crew \{/);
  // A slot there is, a name that isn't one, and nothing to write are refused.
  await assert.rejects(planSlot(root, ['permissions']), /taken/);
  await assert.rejects(planSlot(root, ['Badges']), /not a name/);
  await assert.rejects(
    planSlot(root, ['x', '--backend-only', '--frontend-only']),
    /nothing to write/,
  );
});

test('everything written is formatted as the repository formats it, with no placeholder left', async () => {
  const plans = [
    await feature(
      'parking_lot',
      '--api',
      '--tables',
      'dsp',
      '--page',
      '--settings',
      '--people',
      'paycom',
      '--label',
      'Parking Lot',
    ),
    await feature('notes', '--tab-of', 'timecard', '--mandatory'),
    await feature('desk', '--api'),
    await collector('fleet'),
    await tool('approve_meal_break', '--features', 'timecard.meal_breaks', '--changes'),
    await tool('review_routes', '--scope', 'dsps', '--actions', 'list,flag', '--changing', 'flag'),
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

test('new collector writes its connection, one collection with a fixture, a card and a native test in a shard of its own', async () => {
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
  assert.match(plan.notes.join('\n'), /`dispatchdev generate` writes [^\n]*the frontend's list/);
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

test('new integration writes one in a folder of its own, listed where its owner lists them', async () => {
  const own = (await planIntegration(root, ['post_reminders', '--hears', 'message.channels'])).plan;
  assert(own.files.has('features/slack/integrations/post_reminders/api.rs'));
  assert.match(changed(own, 'features/slack/integrations/mod.rs'), /&post_reminders::OWN/);
  assert.match(
    changed(own, 'features/slack/feature.rs'),
    /integrations::post_reminders::INTEGRATION,/,
  );
  assert.match(
    changed(own, 'features/slack/tests/api/routes.txt'),
    /^GET \/api\/dsp\/slack\/workspaces\/\{team\}\/post-reminders Dsp\("slack\.manage"\) Read$/m,
  );
  // Another feature's is its declaration and its card, in its own integrations/.
  const other = (await planIntegration(root, ['notes', '--to', 'uniforms'])).plan;
  assert.match(file(other, 'features/uniforms/integrations/notes/mod.rs'), /"uniforms\.notes"/);
  const manifest = changed(other, 'features/uniforms/feature.rs');
  assert.match(manifest, /^mod integrations;$/m);
  assert.match(manifest, /integrations: &\[integrations::notes::INTEGRATION\],/);
  // It works through Slack, which its feature declares and its crate lists, as the registry
  // and the dependency rule hold them to.
  assert.match(manifest, /depends_on: &\["slack"\],/);
  assert.match(
    changed(other, 'features/uniforms/Cargo.toml'),
    /^dispatch-slack = \{ path = "\.\.\/slack" \}$/m,
  );
  // Its tests install it with Slack, as the registry wants each dependency installed.
  assert.match(
    changed(other, 'features/uniforms/tests/backend/storage.rs'),
    /install\(&\[[^\]]*\], &\[[^\]]*&dispatch_slack::FEATURE\]/,
  );
  assert.match(
    changed(other, 'features/uniforms/frontend/feature.ts'),
    /connectionIntegrations: \[notes\]/,
  );
  await assert.rejects(planIntegration(root, ['notes', '--to', 'nowhere']), /--to names a feature/);
  await assert.rejects(planIntegration(root, ['notes', '--hears', 'File Shared']), /--hears names/);
});
