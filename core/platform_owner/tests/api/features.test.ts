import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../support/support.js';

type Role = { id: string; name: string; permissions: string[] };
const tabs = [
  'timecard.daily',
  'timecard.meal_breaks',
  'timecard.employees',
  'dvic.day',
  'dvic.week',
];
const all = [
  'timecard',
  'uniforms',
  'routes',
  'dvic',
  'scorecard',
  'driver_match',
  ...tabs,
  'paycom',
  'cortex',
];
/** The DSP's features with these pages and connections on: every tab of a page that is. */
const having = (...ids: string[]) =>
  all.filter((f) => ids.includes(f) || (tabs.includes(f) && ids.includes(f.split('.')[0]!)));

test('a feature switched off for a DSP stops existing there until it is switched back on', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const platform = await f.client();
  const member = await f.client('member@dispatch.test');
  const north = member.session.dsps[0];
  assert.deepEqual(north.features, all);
  let view = await member.select(north.id);
  assert.deepEqual(view.features, all);
  assert.deepEqual(view.permissions, ['uniforms.view', 'timecard.view']);
  assert.equal((await member.get('/api/dsp/uniforms')).status, 200);

  const url = `/api/platform/dsps/${north.id}/features`;
  // The platform's page reads each feature's state with what a switch would stop.
  assert.equal((await member.get(url)).status, 403);
  const report = (await platform.get(url)).value;
  assert.deepEqual(
    report.features.map((state: { feature: string; enabled: boolean }) => [
      state.feature,
      state.enabled,
    ]),
    all.map((feature) => [feature, true]),
  );
  assert.equal(typeof report.features[0].changedAt, 'string');
  assert.deepEqual([report.schedules, report.activeJobs], [0, 0]);
  assert.equal((await member.post(url, { feature: 'uniforms', enabled: false })).status, 403);
  assert.equal((await platform.post(url, { feature: 'nothing', enabled: false })).status, 404);
  assert.equal((await platform.post(url, { feature: 'uniforms' })).status, 400);
  let result = await platform.post(url, { feature: 'uniforms', enabled: false });
  assert.equal(result.status, 200);
  assert.deepEqual(result.value, {
    features: having('timecard', 'routes', 'dvic', 'scorecard', 'driver_match', 'paycom', 'cortex'),
    changed: [{ feature: 'uniforms', enabled: false }],
  });
  // Every open view of the DSP expires; reopened, it has no uniform inventory.
  assert.equal((await member.get('/api/dsp/uniforms')).value.error, 'dsp_view_expired');
  view = await member.select(north.id);
  assert.deepEqual(
    view.features,
    having('timecard', 'routes', 'dvic', 'scorecard', 'driver_match', 'paycom', 'cortex'),
  );
  assert.deepEqual(view.permissions, ['timecard.view']);
  assert.equal((await member.get('/api/dsp/uniforms')).status, 403);
  // Owners hold every permission, but only of the features the DSP has.
  const owner = await platform.select(north.id);
  assert.deepEqual(
    owner.features,
    having('timecard', 'routes', 'dvic', 'scorecard', 'driver_match', 'paycom', 'cortex'),
  );
  assert.ok(!owner.permissions.includes('uniforms.view'));
  assert.equal((await platform.get('/api/dsp/uniforms')).status, 403);
  const listed = (await platform.get('/api/platform/dsps')).value.find(
    (dsp: { id: string }) => dsp.id === north.id,
  );
  assert.deepEqual(
    listed.features,
    having('timecard', 'routes', 'dvic', 'scorecard', 'driver_match', 'paycom', 'cortex'),
  );

  // A role keeps the grant it cannot show, through a save from the role sheet too.
  const roles: Role[] = (await platform.get('/api/dsp/roles')).value;
  const memberRole = roles.find((role) => role.name === 'Member')!;
  assert.deepEqual(memberRole.permissions, ['uniforms.view', 'timecard.view']);
  const saved = await platform.post(`/api/dsp/roles/${memberRole.id}`, {
    name: 'Member',
    permissions: ['timecard.manage'],
  });
  assert.equal(saved.status, 200);
  assert.deepEqual(saved.value.permissions, ['uniforms.view', 'timecard.view', 'timecard.manage']);

  // Switching it back on restores everything as it was.
  result = await platform.post(url, { feature: 'uniforms', enabled: true });
  assert.deepEqual(result.value.changed, [{ feature: 'uniforms', enabled: true }]);
  view = await member.select(north.id);
  assert.deepEqual(view.permissions, ['uniforms.view', 'timecard.view', 'timecard.manage']);
  assert.equal((await member.get('/api/dsp/uniforms')).status, 200);
  result = await platform.post(url, { feature: 'uniforms', enabled: true });
  assert.deepEqual(result.value.changed, []);
  const restored = (await platform.get(url)).value.features.find(
    (state: { feature: string }) => state.feature === 'uniforms',
  );
  assert.equal(restored.changedBy, 'Platform Owner');

  // A connection is its own feature; the pages requiring what it provides go with it.
  result = await platform.post(url, { feature: 'cortex', enabled: false });
  assert.deepEqual(result.value, {
    features: having('uniforms', 'paycom'),
    changed: [
      { feature: 'cortex', enabled: false },
      { feature: 'timecard', enabled: false },
      { feature: 'routes', enabled: false },
      { feature: 'dvic', enabled: false },
      { feature: 'scorecard', enabled: false },
      { feature: 'driver_match', enabled: false },
    ],
  });
  view = await member.select(north.id);
  assert.deepEqual(view.permissions, ['uniforms.view']);
  assert.equal((await member.get('/api/dsp/employees')).status, 403);
  await platform.select(north.id);
  assert.equal((await platform.get('/api/dsp/connections/cortex')).status, 404);
  assert.equal((await platform.get('/api/dsp/connections/paycom')).status, 200);
  assert.equal((await platform.get('/api/dsp/schedules')).status, 403);
  // Enabling a page enables what it requires, and only that page.
  result = await platform.post(url, { feature: 'timecard', enabled: true });
  assert.deepEqual(result.value, {
    features: having('timecard', 'uniforms', 'paycom', 'cortex'),
    changed: [
      { feature: 'cortex', enabled: true },
      { feature: 'timecard', enabled: true },
    ],
  });
  await platform.select(north.id);
  assert.equal((await platform.get('/api/dsp/routes/days')).status, 403);
  result = await platform.post(url, { feature: 'routes', enabled: true });
  assert.deepEqual(result.value, {
    features: having('timecard', 'uniforms', 'routes', 'paycom', 'cortex'),
    changed: [{ feature: 'routes', enabled: true }],
  });
  await platform.select(north.id);
  assert.equal((await platform.get('/api/dsp/connections/cortex')).status, 200);
  // Without any connection, nobody manages connections: the permission is gone too.
  await platform.post(url, { feature: 'paycom', enabled: false });
  result = await platform.post(url, { feature: 'cortex', enabled: false });
  assert.deepEqual(result.value.features, ['uniforms']);
  const alone = await platform.select(north.id);
  assert.ok(!alone.permissions.includes('connections.manage'));
  assert.equal((await platform.get('/api/dsp/connections')).status, 403);

  // Each switch is the platform's own record, never the DSP's.
  const events = (await platform.get('/api/platform/audit?limit=20')).value.events;
  const switches = events
    .filter((event: { action: string }) => event.action.startsWith('dsp.feature_'))
    .map((event: { action: string; detail: string }) => [event.action, event.detail]);
  // The last switch and its cascade were written in the same instant.
  assert.deepEqual(switches.slice(0, 4).sort(), [
    ['dsp.feature_disabled', 'cortex'],
    ['dsp.feature_disabled', 'paycom'],
    ['dsp.feature_disabled', 'routes'],
    ['dsp.feature_disabled', 'timecard'],
  ]);
  // The timecard last went with Paycom; a cascade names what took it.
  const cascade = events.find(
    (event: { action: string; detail: string }) =>
      event.action === 'dsp.feature_disabled' && event.detail === 'timecard',
  );
  assert.deepEqual(cascade.changes, [{ field: 'cause', from: null, to: 'Paycom' }]);
  // Feature switches still work after verification ages; destructive removal does not.
  f.database('data/platform/accounts.sqlite', (db) =>
    db.prepare('UPDATE sessions SET created_at=0 WHERE user_id=?').run(platform.session.user.id),
  );
  assert.equal((await platform.post(url, { feature: 'uniforms', enabled: false })).status, 200);
  assert.equal((await platform.post(url, { feature: 'uniforms', enabled: true })).status, 200);
  const remove = await platform.post(`/api/platform/dsps/${north.id}/remove`, {});
  assert.equal(remove.status, 403, remove.body);
  assert.ok(
    ['sign_in_again', 'reauthentication_required'].includes(remove.value.error),
    remove.body,
  );
});

test('a tab switched off has no routes, and a page goes and comes back with its tabs', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const platform = await f.client();
  const member = await f.client('member@dispatch.test');
  const north = member.session.dsps[0];
  const url = `/api/platform/dsps/${north.id}/features`;
  const meals = '/api/dsp/paycom/meal-breaks?date=2026-01-05';
  await member.select(north.id);
  assert.equal((await member.get('/api/dsp/employees')).status, 200);
  let result = await platform.post(url, { feature: 'timecard.employees', enabled: false });
  assert.deepEqual(result.value.changed, [{ feature: 'timecard.employees', enabled: false }]);
  // The page and its permissions stay; only the tab and its routes are gone.
  let view = await member.select(north.id);
  assert.deepEqual(
    view.features,
    all.filter((feature) => feature !== 'timecard.employees'),
  );
  assert.deepEqual(view.permissions, ['uniforms.view', 'timecard.view']);
  assert.equal((await member.get('/api/dsp/employees')).status, 404);
  assert.equal((await member.get(meals)).status, 200);
  assert.equal((await member.get('/api/dsp/timecards?date=2026-01-05')).status, 200);

  // The page's last tab takes the page along, which keeps its tabs' switches.
  result = await platform.post(url, { feature: 'timecard.meal_breaks', enabled: false });
  assert.deepEqual(result.value.changed, [{ feature: 'timecard.meal_breaks', enabled: false }]);
  result = await platform.post(url, { feature: 'timecard.daily', enabled: false });
  assert.deepEqual(result.value.changed, [
    { feature: 'timecard.daily', enabled: false },
    { feature: 'timecard', enabled: false },
  ]);
  view = await member.select(north.id);
  assert.deepEqual(
    view.features,
    having('uniforms', 'routes', 'dvic', 'scorecard', 'driver_match', 'paycom', 'cortex'),
  );
  assert.deepEqual(view.permissions, ['uniforms.view']);
  const report: { feature: string; enabled: boolean }[] = (await platform.get(url)).value.features;
  assert.deepEqual(
    report.filter((s) => s.feature.startsWith('timecard')).map((s) => [s.feature, s.enabled]),
    [
      ['timecard', false],
      ['timecard.daily', false],
      ['timecard.meal_breaks', false],
      ['timecard.employees', false],
    ],
  );
  // Switched back on without a tab, the page brings every tab.
  result = await platform.post(url, { feature: 'timecard', enabled: true });
  assert.deepEqual(result.value.changed, [
    { feature: 'timecard', enabled: true },
    { feature: 'timecard.daily', enabled: true },
    { feature: 'timecard.meal_breaks', enabled: true },
    { feature: 'timecard.employees', enabled: true },
  ]);
  await member.select(north.id);
  assert.equal((await member.get('/api/dsp/employees')).status, 200);
  // A tab's page is named with it, and names the cause of the page's switch.
  const events = (await platform.get('/api/platform/audit?limit=20')).value.events;
  const page = events.find(
    (event: { action: string; detail: string }) =>
      event.action === 'dsp.feature_disabled' && event.detail === 'timecard',
  );
  assert.deepEqual(page.changes, [{ field: 'cause', from: null, to: 'Timecard · Timecard' }]);
});
