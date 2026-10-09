import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../core/shell/tests/support/support.js';
import { featureCatalog } from '../../../core/tenancy/api/generated/access-catalog.js';

type Role = { id: string; name: string; permissions: string[] };
// Every feature this build has, in the catalog's order: pages, their parts, the connections.
const all: string[] = featureCatalog.map((feature) => feature.id);
/** `features` without `gone`, and without the parts of a page among them. */
const without = (features: readonly string[], ...gone: string[]) =>
  features.filter((id) => {
    const entry = featureCatalog.find((feature) => feature.id === id);
    return !gone.includes(id) && !(entry?.kind === 'sub' && gone.includes(entry.page));
  });

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
  // What every DSP has has no switch.
  const fixed = await platform.post(url, { feature: 'team', enabled: false });
  assert.deepEqual([fixed.status, fixed.value.error], [409, 'feature_mandatory']);
  let result = await platform.post(url, { feature: 'uniforms', enabled: false });
  assert.equal(result.status, 200);
  assert.deepEqual(result.value, {
    features: without(all, 'uniforms'),
    changed: [{ feature: 'uniforms', enabled: false }],
  });
  // Every open view of the DSP expires; reopened, it has no uniform inventory.
  assert.equal((await member.get('/api/dsp/uniforms')).value.error, 'dsp_view_expired');
  view = await member.select(north.id);
  assert.deepEqual(view.features, without(all, 'uniforms'));
  assert.deepEqual(view.permissions, ['timecard.view']);
  assert.equal((await member.get('/api/dsp/uniforms')).status, 403);
  // Owners hold every permission, but only of the features the DSP has.
  const owner = await platform.select(north.id);
  assert.deepEqual(owner.features, without(all, 'uniforms'));
  assert.ok(!owner.permissions.includes('uniforms.view'));
  assert.equal((await platform.get('/api/dsp/uniforms')).status, 403);
  const listed = (await platform.get('/api/platform/dsps')).value.find(
    (dsp: { id: string }) => dsp.id === north.id,
  );
  assert.deepEqual(listed.features, without(all, 'uniforms'));

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

  // A connection is its own feature; the pages and parts requiring what it provides go with
  // it: Timecard stays, without its Meal Breaks tab, which alone needs Cortex.
  result = await platform.post(url, { feature: 'cortex', enabled: false });
  assert.deepEqual(result.value, {
    features: without(
      all,
      'cortex',
      'routes',
      'dvic',
      'weekly_scorecard',
      'driver_match',
      'daily_performance',
      'timecard.meal_breaks',
    ),
    changed: [
      { feature: 'cortex', enabled: false },
      { feature: 'routes', enabled: false },
      { feature: 'dvic', enabled: false },
      { feature: 'weekly_scorecard', enabled: false },
      { feature: 'driver_match', enabled: false },
      { feature: 'daily_performance', enabled: false },
      { feature: 'timecard.meal_breaks', enabled: false },
    ],
  });
  view = await member.select(north.id);
  assert.deepEqual(view.permissions, ['uniforms.view', 'timecard.view', 'timecard.manage']);
  assert.equal((await member.get('/api/dsp/employees')).status, 200);
  assert.equal((await member.get('/api/dsp/paycom/meal-breaks?date=2026-01-05')).status, 404);
  await platform.select(north.id);
  assert.equal((await platform.get('/api/dsp/connections/cortex')).status, 404);
  assert.equal((await platform.get('/api/dsp/connections/paycom')).status, 200);
  // Enabling a part enables what it requires, and only that part.
  result = await platform.post(url, { feature: 'timecard.meal_breaks', enabled: true });
  assert.deepEqual(result.value, {
    features: without(
      all,
      'routes',
      'dvic',
      'weekly_scorecard',
      'driver_match',
      'daily_performance',
    ),
    changed: [
      { feature: 'cortex', enabled: true },
      { feature: 'timecard.meal_breaks', enabled: true },
    ],
  });
  await platform.select(north.id);
  assert.equal((await platform.get('/api/dsp/routes/days')).status, 403);
  result = await platform.post(url, { feature: 'routes', enabled: true });
  assert.deepEqual(result.value, {
    features: without(all, 'dvic', 'weekly_scorecard', 'driver_match', 'daily_performance'),
    changed: [{ feature: 'routes', enabled: true }],
  });
  await platform.select(north.id);
  assert.equal((await platform.get('/api/dsp/connections/cortex')).status, 200);
  // Without any connection, nobody manages connections: the permission is gone too.
  await platform.post(url, { feature: 'paycom', enabled: false });
  result = await platform.post(url, { feature: 'cortex', enabled: false });
  assert.deepEqual(
    result.value.features,
    without(
      all,
      'paycom',
      'cortex',
      'timecard',
      'routes',
      'dvic',
      'weekly_scorecard',
      'driver_match',
      'daily_performance',
    ),
  );
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
  assert.deepEqual(view.features, without(all, 'timecard'));
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

test('a feature hidden from a DSP keeps running, out of sight of all but the platform owner', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const platform = await f.client();
  const member = await f.client('member@dispatch.test');
  const north = member.session.dsps[0];
  const features = `/api/platform/dsps/${north.id}/features`;
  const url = `${features}/shown`;
  // Whether the feature is on, and whether its members see it.
  const state = async (id: string) => {
    const report = await platform.read(features);
    const found = report.features.find((s: { feature: string }) => s.feature === id);
    return [found.enabled, found.shown];
  };
  assert.deepEqual(await state('uniforms'), [true, true]);
  assert.equal((await member.post(url, { feature: 'uniforms', shown: false })).status, 403);
  assert.equal((await platform.post(url, { feature: 'nothing', shown: false })).status, 404);
  assert.equal((await platform.post(url, { feature: 'uniforms' })).status, 400);
  // What every DSP has is always shown, and a connection has nothing to show.
  const fixed = await platform.post(url, { feature: 'team', shown: false });
  assert.deepEqual([fixed.status, fixed.value.error], [409, 'feature_mandatory']);
  const connection = await platform.post(url, { feature: 'cortex', shown: false });
  assert.deepEqual([connection.status, connection.value.error], [400, 'invalid_input']);

  await member.select(north.id);
  let result = await platform.post(url, { feature: 'uniforms', shown: false });
  assert.deepEqual([result.status, result.value], [200, { hidden: ['uniforms'] }]);
  // Members lose it at once; it stays switched on.
  assert.equal((await member.get('/api/dsp/uniforms')).value.error, 'dsp_view_expired');
  let view = await member.select(north.id);
  assert.deepEqual(
    view.features,
    all.filter((feature) => feature !== 'uniforms'),
  );
  assert.deepEqual(view.permissions, ['timecard.view']);
  assert.equal((await member.get('/api/dsp/uniforms')).status, 403);
  assert.ok(
    !(await member.read('/api/session')).dsps[0].features.includes('uniforms'),
    'the DSP list hides it too',
  );
  assert.deepEqual(await state('uniforms'), [true, false]);
  // The DSP's owners, as a platform owner previews them, don't see it either.
  const opened = await platform.select(north.id);
  const ownerRole = opened.roles.find((role: { owner: boolean }) => role.owner);
  const preview = await platform.post('/api/session/dsp', {
    dspId: north.id,
    roleId: ownerRole.id,
  });
  platform.headers['x-dispatch-view'] = preview.value.token;
  assert.ok(!preview.value.features.includes('uniforms'));
  assert.ok(!preview.value.permissions.includes('uniforms.view'));
  assert.equal((await platform.get('/api/dsp/uniforms')).status, 403);
  // The platform owner's own view of the DSP does.
  view = await platform.select(north.id);
  assert.equal(view.role.id, 'platform_owner');
  assert.deepEqual(view.features, all);
  assert.ok(view.permissions.includes('uniforms.view'));
  assert.equal((await platform.get('/api/dsp/uniforms')).status, 200);
  const listed = (await platform.read('/api/platform/dsps')).find(
    (dsp: { id: string }) => dsp.id === north.id,
  );
  assert.deepEqual(listed.features, all);

  // A hidden tab takes its routes from sight; a hidden page takes its tabs.
  const meals = '/api/dsp/paycom/meal-breaks?date=2026-01-05';
  result = await platform.post(url, { feature: 'timecard.employees', shown: false });
  assert.deepEqual(result.value.hidden, ['uniforms', 'timecard.employees']);
  await member.select(north.id);
  assert.equal((await member.get('/api/dsp/employees')).status, 404);
  assert.equal((await member.get(meals)).status, 200);
  await platform.post(url, { feature: 'timecard', shown: false });
  view = await member.select(north.id);
  assert.deepEqual(
    view.features,
    all.filter((feature) => feature !== 'uniforms' && !feature.startsWith('timecard')),
  );
  // With the page goes its permission, as when it is switched off.
  assert.deepEqual(view.permissions, []);
  assert.equal((await member.get(meals)).status, 403);
  // Shown again, each comes back as it was, the tab hidden on its own still hidden.
  result = await platform.post(url, { feature: 'timecard', shown: true });
  result = await platform.post(url, { feature: 'uniforms', shown: true });
  assert.deepEqual(result.value.hidden, ['timecard.employees']);
  view = await member.select(north.id);
  assert.deepEqual(
    view.features,
    all.filter((feature) => feature !== 'timecard.employees'),
  );
  assert.deepEqual(view.permissions, ['uniforms.view', 'timecard.view']);
  // Asking for what already is changes nothing: open views stay open.
  result = await platform.post(url, { feature: 'uniforms', shown: true });
  assert.deepEqual(result.value.hidden, ['timecard.employees']);
  assert.equal((await member.get('/api/dsp/uniforms')).status, 200);

  // Each is the platform's own record, never the DSP's.
  const events = (await platform.read('/api/platform/audit?limit=20')).events;
  assert.deepEqual(
    events
      .filter((event: { action: string }) => /^dsp\.feature_(hidden|shown)$/.test(event.action))
      .map((event: { action: string; detail: string }) => [event.action, event.detail])
      .reverse(),
    [
      ['dsp.feature_hidden', 'uniforms'],
      ['dsp.feature_hidden', 'timecard.employees'],
      ['dsp.feature_hidden', 'timecard'],
      ['dsp.feature_shown', 'timecard'],
      ['dsp.feature_shown', 'uniforms'],
    ],
  );
});
