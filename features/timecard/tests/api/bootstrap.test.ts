import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { demo, fixture } from '../../../../core/shell/tests/support/support.js';
const { password } = demo;

test('Rust bootstrap creates only the initial owner and empty Dev DSP; locks exclude another core and backup', async (t) => {
  const f = await fixture(false);
  t.after(f.close);
  const owner = await f.client();
  assert.equal(owner.session.dsps.length, 1);
  await owner.select(owner.session.dsps[0].id);
  assert.equal((await owner.get('/api/dsp/employees')).value.total, 0);
  assert.equal((await owner.get('/api/dsp/connections')).value.enabled, false);
  assert.equal((await owner.get('/api/dsp/jobs')).value.length, 0);
  assert.throws(() => f.cli(['serve']), /stop_services_before_operation/);
  assert.throws(() => f.cli(['backup', `${f.root}-backup`]), /stop_services_before_operation/);
  // The served process is the backend itself, with no children: ps tells both on Linux, which
  // keeps 15 characters of its name, and macOS, which gives its path.
  const ps = (...args: string[]) => execFileSync('ps', args, { encoding: 'utf8' });
  assert.match(path.basename(ps('-o', 'comm=', '-p', String(f.pid())).trim()), /^dispatch-backen/);
  const parents = ps('-A', '-o', 'ppid=')
    .split('\n')
    .map((line) => line.trim());
  assert.ok(!parents.includes(String(f.pid())));
  await f.stop();
  assert.throws(
    () => f.cli(['bootstrap', 'again@dispatch.test', 'Again', 'Owner'], password),
    /bootstrap_requires_empty_platform/,
  );
});
