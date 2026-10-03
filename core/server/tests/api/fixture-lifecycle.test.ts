import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fixture, prepare } from '../support/support.js';

test('failed fixture preparation, spawn and startup remove their private state', async (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-fixture-lifecycle-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const binary = path.join(root, 'fixture-backend');
  const record = path.join(root, 'state-root');
  for (const phase of ['seed', 'spawn', 'serve']) {
    // This executable exercises real fixture subprocess failures without building
    // a second server or depending on the platform's account seed.
    fs.writeFileSync(
      binary,
      `#!${process.execPath}
const fs = require('node:fs');
const phase = process.env.DISPATCH_FIXTURE_TEST_PHASE;
if (process.argv[2] === 'seed') {
  fs.writeFileSync(process.env.DISPATCH_FIXTURE_TEST_RECORD, process.env.DISPATCH_STATE_ROOT);
  if (phase === 'spawn') fs.unlinkSync(process.argv[1]);
  if (phase !== 'seed') process.exit(0);
}
console.error('synthetic fixture ' + phase + ' failure');
process.exit(1);
`,
      { mode: 0o700 },
    );
    const options = {
      binary,
      env: { DISPATCH_FIXTURE_TEST_PHASE: phase, DISPATCH_FIXTURE_TEST_RECORD: record },
    };
    if (phase === 'seed') await assert.rejects(prepare(options), /synthetic fixture seed failure/);
    else
      await assert.rejects(
        fixture(options),
        phase === 'spawn' ? /ENOENT/ : /synthetic fixture serve failure/,
      );
    assert.equal(
      fs.existsSync(fs.readFileSync(record, 'utf8')),
      false,
      `${phase}: private state removed`,
    );
  }
});

test('forced fixture shutdown reaps the child before removing its state', async (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-fixture-stop-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const binary = path.join(root, 'fixture-backend');
  fs.writeFileSync(
    binary,
    `#!${process.execPath}
if (process.argv[2] === 'seed') process.exit(0);
process.on('SIGTERM', () => {});
require('node:http').createServer((_req, res) => res.end('{}'))
  .listen(Number(process.env.PORT), '127.0.0.1', () =>
    console.log(JSON.stringify({event:'core.started'})));
`,
    { mode: 0o700 },
  );
  const f = await fixture({ binary });
  t.after(async () => {
    t.mock.timers.reset();
    await f.stop('SIGKILL');
    await f.close();
  });
  let reaped = false;
  void f.exited().then(() => {
    reaped = true;
  });
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const closing = f.close().catch((error: unknown) => error);
  t.mock.timers.tick(10000);
  const error = await closing;
  assert(error instanceof Error);
  assert.match(error.message, /Server failed to stop:/);
  assert.equal(reaped, true, 'the forced child exit precedes close completion');
  assert.equal(fs.existsSync(f.root), false);
});
