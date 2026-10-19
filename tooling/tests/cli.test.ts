import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { CHECKS, MARKER, described } from '../scaffold/doctor.js';
import { duration, failure, report, writers } from '../scaffold/generate.js';
import { LISTS, explain, fields, frontendSlots, table } from '../scaffold/inventory.js';
import { KINDS } from '../scaffold/cli.js';
import { repositoryRoot as root } from '../scaffold/scaffold.js';

// What `dispatchdev new`, `generate`, `list` and `explain` run: the scaffolder's dispatcher,
// which answers its usage with success, and what it reads. Nothing here builds Rust.

const tsx = path.join(root, 'node_modules/tsx/dist/cli.mjs');
const cli = (...args: string[]) =>
  spawnSync(process.execPath, [tsx, path.join(root, 'tooling/scaffold/cli.ts'), ...args], {
    cwd: root,
    encoding: 'utf8',
  });

test('each command answers its usage with success, and what it does not take with a failure', () => {
  const kinds = cli('new');
  assert.equal(kinds.status, 0, kinds.stderr);
  for (const { kind } of KINDS) assert.match(kinds.stdout, new RegExp(`^  ${kind} `, 'm'));
  const feature = cli('new', 'feature', '--help');
  assert.equal(feature.status, 0, feature.stderr);
  assert.match(feature.stdout, /^Usage: dispatchdev new feature <name>/);
  const lists = cli('list');
  assert.equal(lists.status, 0, lists.stderr);
  for (const name of Object.keys(LISTS)) assert.match(lists.stdout, new RegExp(`^  ${name} `, 'm'));
  assert.equal(cli('generate', '--help').status, 0);
  assert.equal(cli('explain').status, 0);

  const widget = cli('new', 'widget', 'thing');
  assert.equal(widget.status, 1);
  assert.match(
    widget.stderr,
    new RegExp(`dispatchdev new makes ${KINDS.map(({ kind }) => kind).join(', ')}`),
  );
  const wings = cli('new', 'feature', 'desk', '--wings');
  assert.equal(wings.status, 1);
  assert.match(wings.stderr, /Unknown option --wings\n\nUsage: dispatchdev new feature/);
  assert.equal(cli('list', 'nothing').status, 1);
  assert.match(cli('explain', 'nothing').stderr, /No feature or collector nothing/);
});

test('new --dry-run --json answers what it would write, and writes nothing', () => {
  const result = cli('new', 'feature', 'parking', '--page', '--dry-run', '--json');
  assert.equal(result.status, 0, result.stderr);
  const { made } = JSON.parse(result.stdout) as {
    made: { dryRun: boolean; wrote: string[]; changed: string[]; next: string[] };
  };
  assert.equal(made.dryRun, true);
  assert(made.wrote.includes('features/parking/feature.rs'), made.wrote.join('\n'));
  assert(made.changed.includes('app/backend/features.rs'));
  assert(made.next.some((note) => note.includes('`dispatchdev generate`')));
  assert(!fs.existsSync(path.join(root, 'features/parking')));
});

test('generate writes with the module tests that write: the API types and every snapshot', () => {
  const modules = writers(root);
  for (const module of ['export', 'catalog', 'agent_toolbox', 'databases', 'schema'])
    assert(modules.includes(module), `${module} is not among ${modules.join(', ')}`);
  // A module the app mounts under another name runs by that name.
  assert(!modules.includes('audit'));
  assert.equal(duration(45), '45s');
  assert.equal(duration(185), '3m 05s');
  const log =
    'Compiling x\n---- catalog::pages stdout ----\nthread panicked at a.rs:1:2\nleft: 1\n';
  assert.deepEqual(failure(log), ['thread panicked at a.rs:1:2', 'left: 1']);
  assert.match(
    report({
      ok: false,
      steps: [{ name: 'build', ok: false, seconds: 3, log: '/tmp/x.log' }],
      changed: [],
    }),
    /Fix what failed and run dispatchdev generate again: \/tmp\/x\.log/,
  );
  assert.equal(report({ ok: true, steps: [], changed: [] }), 'Nothing generated changed.');
});

test('list and explain read what the build lists', () => {
  for (const [name, list] of Object.entries(LISTS))
    assert(list.rows(root).length > 0, `dispatchdev list ${name} lists nothing`);
  const features = LISTS.features!.rows(root).map((row) => row.name);
  assert(features.includes('slack') && features.includes('driver_match'));
  const slack = explain(root, 'slack');
  assert.equal(slack.folder, 'features/slack');
  assert((slack.permissions as string[]).includes('slack.manage'));
  assert((slack.frontend as string[]).includes('connectionPieces'));
  assert.equal(explain(root, 'cortex').owner, 'collector');
  assert.throws(() => explain(root, 'nothing'), /No feature or collector/);

  assert.deepEqual(
    frontendSlots(
      "export const feature: FrontendFeature = {\n  name: 'x',\n  routes: [\n    { id: 'x' },\n  ],\n  // A note.\n  platformSlots: () => import('./p.js'),\n  get settingsSections() {\n    return [];\n  },\n};\n",
    ),
    ['routes', 'platformSlots', 'settingsSections'],
  );
  assert.equal(
    table([
      { name: 'a', uses: [] },
      { name: 'long', uses: ['x', 'y'] },
    ]),
    'name  uses\na     -\nlong  x, y',
  );
  assert.equal(fields({ name: 'a', uses: ['x'] }), 'name: a\nuses: x');
});

test('doctor names each file a generator left to write, and says what fixes it', () => {
  const checkout = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-doctor-'));
  try {
    spawnSync('git', ['init', '-q'], { cwd: checkout });
    fs.mkdirSync(path.join(checkout, 'features/desk'), { recursive: true });
    fs.writeFileSync(path.join(checkout, 'features/desk/feature.rs'), `//! Desk: ${MARKER}: x\n`);
    fs.writeFileSync(path.join(checkout, 'features/desk/README.md'), '# Desk\n');
    const markers = CHECKS.find((check) => check.name === 'scaffold markers')!.run(checkout);
    assert.deepEqual(markers.detail, ['features/desk/feature.rs']);
    assert.match(
      described([{ check: 'scaffold markers', ...markers }]),
      /^FIX {3}scaffold markers: write what each .+ asks for\n {8}features\/desk\/feature\.rs$/,
    );
  } finally {
    fs.rmSync(checkout, { recursive: true, force: true });
  }
});
