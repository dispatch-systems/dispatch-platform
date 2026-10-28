import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { featureList } from '../../scaffold/wire.js';
import { copyWorkspace, registry, root, temporary } from './support.js';

// The generators run as their own scripts: what --dry-run prints, what they write, and what
// generate --check finds.

const tsx = path.join(root, 'node_modules/tsx/dist/cli.mjs');
// A copy of the repository's workspace, which these tests write into.
let copy = '';
before(() => {
  copy = copyWorkspace();
});
after(() => fs.rmSync(copy, { recursive: true, force: true }));

/** Runs a generator as its own script. */
const generate = (generator: string, args: string[]) =>
  spawnSync(process.execPath, [tsx, path.join(root, `tooling/scaffold/${generator}.ts`), ...args], {
    cwd: root,
    encoding: 'utf8',
  });
/** The files listed under a heading, up to the next. */
const listed = (stdout: string, heading: string) => {
  const lines = stdout.split(`${heading}:\n`)[1]!.split('\n');
  const end = lines.findIndex((line) => !line.startsWith('  '));
  return lines.slice(0, end < 0 ? undefined : end).map((line) => line.trim());
};

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
    assert.match(result.stdout, /`dispatchdev generate` rewrites the snapshots/);
    assert.doesNotMatch(result.stdout, /catalog\.rs|agent-keys|DISPATCH_UPDATE_/);
    assert(!fs.existsSync('features/parking'), 'a dry run wrote into the repository');
    assert.equal(fs.readFileSync(registry, 'utf8'), before);
    const site = generate('new-collector', ['fleet', '--dry-run']);
    assert.equal(site.status, 0, site.stderr);
    assert(listed(site.stdout, 'Would write').includes('collectors/fleet/collector.rs'));
    assert.match(site.stdout, /`dispatchdev generate` rewrites the snapshots/);
    assert.doesNotMatch(site.stdout, /catalog\.rs/);
    assert(!fs.existsSync('collectors/fleet'));
  } finally {
    fs.rmSync(out, { recursive: true, force: true });
  }
});

test('without --dry-run it writes into the root, and refuses to write over an owner', () => {
  const wrote = generate('new-feature', ['desk', '--settings', '--root', copy]);
  assert.equal(wrote.status, 0, wrote.stderr);
  assert(fs.existsSync(path.join(copy, 'features/desk/backend/settings.rs')));
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
  assert.match(usage.stderr, /Unknown option --wings\n\nUsage: dispatchdev new feature/);
});

test('generate --check writes nothing, and fails naming what generate would change', () => {
  const list = path.join(copy, featureList);
  const wired = fs.readFileSync(list, 'utf8');
  const stale = `${wired}// A line the wiring doesn't write.\n`;
  fs.writeFileSync(list, stale);
  try {
    const result = spawnSync(
      process.execPath,
      [
        tsx,
        path.join(root, 'tooling/scaffold/cli.ts'),
        'generate',
        '--check',
        '--json',
        '--root',
        copy,
      ],
      { cwd: root, encoding: 'utf8' },
    );
    assert.equal(result.status, 1, result.stderr);
    const checked = JSON.parse(result.stdout) as {
      ok: boolean;
      steps: { ok: boolean }[];
      changed: string[];
      next: string[];
    };
    assert.equal(checked.ok, false);
    assert(checked.steps.every((step) => step.ok));
    assert.deepEqual(checked.changed, [featureList]);
    assert.match(checked.next.join('\n'), /`dispatchdev generate` writes it/);
    assert.equal(fs.readFileSync(list, 'utf8'), stale);
  } finally {
    fs.writeFileSync(list, wired);
  }
});
