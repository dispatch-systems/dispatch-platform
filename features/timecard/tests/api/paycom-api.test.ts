import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fixture, until } from '../../../../core/shell/tests/support/support.js';
import { localDate } from '../../../../core/shell/frontend/lib/day.js';
import { clockTime, paycomApi } from '../../../../collectors/paycom/tests/support/paycom-api.js';

type Check = { id: string; state: string; error?: string };

// The server reaches a stand-in for Paycom's API and has no browser to start: a connection
// that asked for one would fail here.
test("a DSP connected through Paycom's API is checked function by function, then collects its timecards with no browser", async (t) => {
  // Where Summit Delivery is.
  const zone = 'America/Denver';
  const today = localDate(zone);
  const at = (clock: string) => clockTime(today, clock, zone);
  const api = await paycomApi({
    sid: 'SID123',
    token: 'synthetic-token',
    employees: [
      {
        code: 'A001',
        name: 'Morgan, Avery',
        department: 'Delivery',
        station: 'DCK6',
        position: 'Delivery associate',
        punches: [
          { at: at('08:00'), type: 'ID' },
          { at: at('12:00'), type: 'OL' },
          { at: at('12:30'), type: 'IL' },
          { at: at('17:00'), type: 'OD' },
        ],
      },
      {
        code: 'A002',
        name: 'Ellis, Jordan',
        department: 'Delivery',
        station: 'DCK6',
        position: 'Delivery associate',
        punches: [{ at: at('09:00'), type: 'ID' }],
      },
      {
        code: 'A003',
        name: 'Reed, Morgan',
        department: 'Operations',
        station: 'DCK6',
        position: 'Dispatcher',
        punches: [],
      },
    ],
  });
  t.after(api.close);
  const f = await fixture({ env: { DISPATCH_FIXTURE_PROVIDER_URL: api.url } });
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: { name: string }) => d.name === 'Summit Delivery');
  assert.equal(dsp.timezone, zone);
  await owner.select(dsp.id);
  const connect = (credentials: object) => owner.post('/api/dsp/connections/paycom', credentials);
  /** Each check of the connection, by how it went. */
  const checks = async () =>
    Object.fromEntries(
      (await owner.read('/api/dsp/connections/paycom')).checks.map((check: Check) => [
        check.id,
        check.error ?? check.state,
      ]),
    );

  // A Token Paycom doesn't take: nothing further is asked.
  const refused = await connect({ sid: 'SID123', token: 'another-token' });
  assert.deepEqual([refused.status, refused.value.error], [409, 'paycom_api_refused']);
  assert.deepEqual(await checks(), {
    credentials: 'paycom_api_refused',
    employee_directory: 'waiting',
    employee: 'waiting',
    punch_history: 'waiting',
    labor_allocation_categories: 'waiting',
    pay_period: 'waiting',
  });

  // A function left off, and no pay period to count from: the card can name both.
  api.off.add('punchhistory');
  const off = await connect({ sid: 'SID123', token: 'synthetic-token' });
  assert.deepEqual([off.status, off.value.error], [409, 'paycom_api_function_off']);
  assert.deepEqual(await checks(), {
    credentials: 'passed',
    employee_directory: 'passed',
    employee: 'passed',
    punch_history: 'paycom_api_function_off',
    labor_allocation_categories: 'passed',
    pay_period: 'paycom_period_required',
  });
  assert.equal((await owner.read('/api/dsp/connections/paycom')).status, 'error');

  // Everything on, and a Sunday its pay periods are counted from.
  api.off.clear();
  const sunday = new Date(`${today}T00:00:00Z`);
  sunday.setUTCDate(sunday.getUTCDate() - sunday.getUTCDay() - 14);
  const ready = await connect({
    sid: 'SID123',
    token: 'synthetic-token',
    periodStart: sunday.toISOString().slice(0, 10),
  });
  assert.equal(ready.status, 200, ready.body);
  assert.equal(ready.value.status, 'ready');
  assert.deepEqual(new Set(Object.values(await checks())), new Set(['passed']));
  const vault = fs.readFileSync(path.join(f.root, 'dsps', dsp.id, 'secrets/paycom.enc'), 'utf8');
  assert(!vault.includes('synthetic-token') && !vault.includes('SID123'));

  const job = await owner.post('/api/dsp/jobs', { requestId: 'api' });
  assert.equal(job.status, 202, job.body);
  let finished: { status: string; error?: string; metrics: { method?: string }[] };
  await until(async () => {
    [finished] = await owner.read('/api/dsp/jobs');
    return ['succeeded', 'failed'].includes(finished.status);
  });
  assert.equal(finished!.status, 'succeeded', JSON.stringify(finished!));
  assert.equal(finished!.metrics[0].method, 'api');
  // The roster came a page at a time, each employee's punches from their own history.
  assert(api.answered.some((request) => request.startsWith('v1/employeedirectory?eestatus=A')));
  assert(api.answered.some((request) => request.includes('page=2')));
  assert.equal((await owner.get('/api/dsp/employees')).value.total, 3);
  const { rows } = (await owner.get(`/api/dsp/timecards?date=${today}`)).value;
  const card = (code: string) =>
    rows.find((row: { employeeCode: string }) => row.employeeCode === code);
  assert.equal(card('A001').hours, 8.5);
  assert.deepEqual(
    card('A001').punches.map((punch: { in: string; out: string }) => [punch.in, punch.out]),
    [
      ['08:00 AM', '12:00 PM'],
      ['12:30 PM', '05:00 PM'],
    ],
  );
  assert.equal(card('A002').status, 'Missing punch');
});
