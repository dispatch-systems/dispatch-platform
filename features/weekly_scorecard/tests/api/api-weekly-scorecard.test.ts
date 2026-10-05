import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, until } from '../../../../core/shell/tests/support/support.js';

const cortex = { username: 'fixture@example.test', password: 'fixture-password' };

test('a scorecard week is collected on request, stored per dataset and listed, and a scorecard schedule needs a station', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: any) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  assert.equal((await owner.post('/api/dsp/connections/cortex', cortex)).value.status, 'ready');
  // The weeks view starts empty and names the week a collection targets by default.
  const before = (await owner.get('/api/dsp/weekly-scorecard/weeks')).value;
  assert.deepEqual(before.weeks, []);
  assert.match(before.latestWeek, /^\d{4}-W\d{2}$/);
  // Every request carries the DSP's station.
  assert.equal(
    (
      await owner.post('/api/dsp/profile', {
        name: dsp.name,
        abbreviation: 'NLL',
        stationCode: 'TST1',
        timezone: dsp.timezone,
      })
    ).status,
    200,
  );
  await owner.select(dsp.id);
  assert.equal(
    (await owner.post('/api/dsp/weekly-scorecard/collect', { requestId: 'bad', week: '2026-W54' }))
      .status,
    400,
  );
  const queued = await owner.post('/api/dsp/weekly-scorecard/collect', {
    requestId: 'week-38',
    week: '2026-W38',
  });
  assert.equal(queued.status, 202, queued.body);
  assert.equal(queued.value.kind, 'cortex.weekly_scorecard.collect');
  assert.equal(
    (await owner.post('/api/dsp/scorecard/collect', { requestId: 'week-38', week: '2026-W38' }))
      .value.id,
    queued.value.id,
  );
  assert.equal(
    (
      await owner.post('/api/dsp/weekly-scorecard/collect', {
        requestId: 'week-38',
        week: '2026-W38',
      })
    ).value.id,
    queued.value.id,
  );
  // Its jobs are the scorecard's own: listed by its routes, never the timecard's.
  const jobOf = async (id: string) =>
    (await owner.read('/api/dsp/weekly-scorecard/jobs')).find((j: any) => j.id === id);
  await until(async () => {
    const job = await jobOf(queued.value.id);
    assert.notEqual(job.status, 'failed', JSON.stringify(job));
    return job.status === 'succeeded';
  });
  assert.ok(!(await owner.read('/api/dsp/jobs')).some((j: any) => j.id === queued.value.id));
  const job = await jobOf(queued.value.id);
  assert.equal(job.metrics.at(-1).rows, 19);
  const weeks = (await owner.get('/api/dsp/weekly-scorecard/weeks')).value;
  assert.deepEqual((await owner.get('/api/dsp/scorecard/weeks')).value, weeks);
  assert.deepEqual(
    (await owner.read('/api/dsp/scorecard/jobs')).map((j: any) => j.id),
    (await owner.read('/api/dsp/weekly-scorecard/jobs')).map((j: any) => j.id),
  );
  assert.equal(weeks.weeks.length, 1);
  const week = weeks.weeks[0];
  assert.equal(week.week, '2026-W38');
  assert.equal(week.posted, true);
  const made = await owner.post('/api/platform/agents/keys', {
    name: 'Weekly scorecard compatibility',
    allDsps: false,
    dsps: [dsp.id],
    access: 'read',
    reads: { areas: ['weekly_scorecard'], bypass: false },
    dspReads: [],
    expiresAt: null,
  });
  assert.equal(made.status, 200, made.body);
  const headers = { authorization: `Bearer ${made.value.token}` };
  const canonical = await f.request('/api/v1/weekly-scorecard?week=2026-W38', undefined, headers);
  const legacy = await f.request('/api/v1/scorecard?week=2026-W38', undefined, headers);
  assert.equal(canonical.status, 200, canonical.body);
  assert.deepEqual(legacy.value, canonical.value);
  const rpc = async (method: string, params: object) => {
    // MCP keys are native-client credentials; the browser fixture adds an Origin header.
    const response = await fetch(`http://127.0.0.1:${f.env.PORT}/api/v1/mcp`, {
      method: 'POST',
      headers: {
        ...headers,
        'content-type': 'application/json',
        accept: 'application/json, text/event-stream',
        'mcp-protocol-version': '2025-06-18',
      },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
    });
    const body = await response.text();
    assert.equal(response.status, 200, body);
    return JSON.parse(body).result;
  };
  const listed = await rpc('tools/list', {});
  assert.equal(listed.tools.filter((tool: any) => tool.name === 'weekly_scorecard').length, 1);
  assert.ok(listed.tools.every((tool: any) => tool.name !== 'scorecard'));
  const weekly = await rpc('tools/call', {
    name: 'weekly_scorecard',
    arguments: { week: '2026-W38' },
  });
  const old = await rpc('tools/call', { name: 'scorecard', arguments: { week: '2026-W38' } });
  assert.equal(weekly.isError, false);
  assert.deepEqual(old.structuredContent, weekly.structuredContent);
  assert.deepEqual(weekly.structuredContent, canonical.value);
  assert.equal(week.publication.rowCount, 19);
  assert.equal(
    week.publication.datasets.find((d: any) => d.table === 'returns_to_station').rows,
    2,
  );
  assert.equal(week.publication.datasets.find((d: any) => d.table === 'pickup_failures').rows, 0);
  const stored = f.database(`dsps/${dsp.id}/data/scorecard/scorecard.sqlite`, (db) => ({
    identity: db
      .prepare('SELECT provider,source FROM storage_identity')
      .all()
      .map((r) => ({ ...r })),
    returns: db
      .prepare(
        "SELECT tracking_id,impact,json_extract(row,'$.rts_reason_code') reason FROM returns_to_station ORDER BY row_index",
      )
      .all()
      .map((r) => ({ ...r })),
  }));
  assert.deepEqual(stored.identity, [{ provider: 'cortex', source: 'scorecard-v1' }]);
  assert.deepEqual(stored.returns, [
    { tracking_id: 'TBA000000000001', impact: 1, reason: 'BUSINESS CLOSED' },
    { tracking_id: 'TBA000000000002', impact: 0, reason: 'CUSTOMER UNAVAILABLE' },
  ]);
  // The scorecard's permissions are its own: viewing the timecard grants none of them.
  const member = await f.client('member@dispatch.test');
  await member.select(dsp.id);
  assert.equal((await member.get('/api/dsp/weekly-scorecard/weeks')).status, 403);
  assert.equal((await member.get('/api/dsp/scorecard/weeks')).status, 403);
  const roles = (await owner.get('/api/dsp/roles')).value;
  const memberRole = roles.find((role: any) => role.name === 'Member');
  assert.equal(
    (
      await owner.post(`/api/dsp/roles/${memberRole.id}`, {
        name: 'Member',
        permissions: [...memberRole.permissions, 'weekly_scorecard.view'],
      })
    ).status,
    200,
  );
  await member.select(dsp.id);
  await owner.select(dsp.id);
  assert.equal((await member.get('/api/dsp/weekly-scorecard/weeks')).status, 200);
  assert.equal(
    (await member.post('/api/dsp/weekly-scorecard/collect', { requestId: 'member' })).status,
    403,
  );

  // A scorecard schedule is its own collection, behind its own routes.
  const body = {
    name: 'Weekly Scorecard',
    collection: 'weekly_scorecard',
    cadence: 'daily',
    intervalMinutes: null,
    localTime: '07:00',
    enabled: true,
  };
  assert.equal((await owner.post('/api/dsp/schedules', body)).status, 403);
  const schedule = await owner.post('/api/dsp/weekly-scorecard/schedules', body);
  assert.equal(schedule.status, 201, schedule.body);
  assert.equal(schedule.value.collection, 'weekly_scorecard');
  assert.deepEqual(
    (await owner.get('/api/dsp/scorecard/schedules')).value,
    (await owner.get('/api/dsp/weekly-scorecard/schedules')).value,
  );
  assert.equal(
    (await owner.post('/api/dsp/weekly-scorecard/schedules', { ...body, collection: 'paycom' }))
      .status,
    403,
  );
  assert.ok(
    !(await owner.get('/api/dsp/schedules')).value.schedules.some(
      (s: any) => s.id === schedule.value.id,
    ),
  );
  const scheduled = async () =>
    (await owner.get('/api/dsp/weekly-scorecard/schedules')).value.schedules.find(
      (s: any) => s.id === schedule.value.id,
    );

  // The timecard switched off leaves the scorecard as it was.
  const switches = `/api/platform/dsps/${dsp.id}/features`;
  await owner.post(switches, { feature: 'timecard', enabled: false });
  await owner.select(dsp.id);
  assert.equal((await owner.get('/api/dsp/weekly-scorecard/weeks')).status, 200);
  assert.equal((await scheduled()).enabled, true);
  await owner.post(switches, { feature: 'timecard', enabled: true });
  await owner.select(dsp.id);

  // Switched off, the scorecard stops: its weeks, its collections, its schedules and the
  // jobs waiting to collect it.
  const waiting = await owner.post('/api/dsp/weekly-scorecard/collect', {
    requestId: 'week-37',
    week: '2026-W37',
  });
  assert.equal(waiting.status, 202, waiting.body);
  await until(async () => (await jobOf(waiting.value.id)).status === 'succeeded');
  await f.stop();
  f.database('data/preview/jobs.sqlite', (db) =>
    db
      .prepare(
        "UPDATE jobs SET status='queued',completed_at=NULL,lease_owner=NULL,lease_until=NULL,available_at=? WHERE id=?",
      )
      .run(4102444800000, waiting.value.id),
  );
  await f.start();
  await owner.select(dsp.id);
  // Only its own routes cancel it.
  assert.equal((await owner.post(`/api/dsp/jobs/${waiting.value.id}/cancel`, {})).status, 403);
  const off = await owner.post(switches, { feature: 'weekly_scorecard', enabled: false });
  assert.deepEqual(off.value.changed, [{ feature: 'weekly_scorecard', enabled: false }]);
  await owner.select(dsp.id);
  for (const url of [
    '/api/dsp/scorecard/weeks',
    '/api/dsp/scorecard/jobs',
    '/api/dsp/scorecard/schedules',
    '/api/dsp/weekly-scorecard/weeks',
    '/api/dsp/weekly-scorecard/jobs',
    '/api/dsp/weekly-scorecard/schedules',
  ])
    assert.equal((await owner.get(url)).status, 403, url);
  assert.equal(
    (await owner.post('/api/dsp/weekly-scorecard/collect', { requestId: 'switched-off' })).status,
    403,
  );
  // Switched back on, everything is as it was, less the job the switch cancelled.
  await owner.post(switches, { feature: 'weekly_scorecard', enabled: true });
  await owner.select(dsp.id);
  assert.equal((await jobOf(waiting.value.id)).status, 'cancelled');
  assert.deepEqual(
    (await owner.get('/api/dsp/weekly-scorecard/weeks')).value.weeks.map((w: any) => w.week),
    ['2026-W38', '2026-W37'],
  );
  assert.equal((await scheduled()).enabled, true);

  assert.equal(
    (await owner.post('/api/dsp/connections/cortex/disable', { removeCredentials: false })).status,
    200,
  );
  assert.equal((await scheduled()).enabled, false);
});
