import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
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
  assert.equal(fs.readFileSync(`/proc/${f.pid()}/comm`, 'utf8').trim(), 'dispatch-backen');
  assert.equal(fs.readFileSync(`/proc/${f.pid()}/task/${f.pid()}/children`, 'utf8').trim(), '');
  await f.stop();
  assert.throws(
    () => f.cli(['bootstrap', 'again@dispatch.test', 'Again', 'Owner'], password),
    /bootstrap_requires_empty_platform/,
  );
});
