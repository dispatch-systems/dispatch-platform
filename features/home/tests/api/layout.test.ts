import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../../core/shell/tests/support/support.js';

const LAYOUT = '/api/dsp/preferences/home.layout';

test("a member's Home layout is kept as their own and comes with their view, only when each widget fits the grid", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const member = await f.client('member@dispatch.test');
  const dsp = member.session.dsps[0].id;
  assert.equal((await member.select(dsp)).preferences['home.layout'], undefined);
  const layout = { widgets: [{ id: 'vans.fleet', x: 6, y: 0, w: 6, h: 3 }] };
  assert.deepEqual((await member.post(LAYOUT, { revision: 0, value: layout })).value, {
    outcome: 'saved',
    revision: 1,
  });
  assert.deepEqual((await member.select(dsp)).preferences['home.layout'], {
    revision: 1,
    value: layout,
  });
  const offGrid = { widgets: [{ id: 'vans.fleet', x: 9, y: 0, w: 6, h: 3 }] };
  assert.equal((await member.post(LAYOUT, { revision: 1, value: offGrid })).status, 400);
});
