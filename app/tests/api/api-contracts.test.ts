import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../core/shell/tests/support/support.js';
import '../support/manifests.js';
import { parseApiResponse } from '../../../core/shell/frontend/runtime/replies.js';
import type { AuditPage, PlatformHealth } from '../../../core/platform_owner/api/index.js';
import type { DspSummary } from '../../../core/accounts/api/index.js';
import type { PaycomSettings } from '../../../features/timecard/api/index.js';

test('recovery-code responses accept the active and previous rollout formats', async () => {
  const route = '/api/auth/security/recovery-codes';
  for (const code of ['Ab1_-Cd2.-Ef3_-Gh4.', 'A'.repeat(43)])
    assert.deepEqual(await parseApiResponse(route, 'POST', { codes: [code] }), { codes: [code] });
  await assert.rejects(
    parseApiResponse(route, 'POST', { codes: ['A'.repeat(32)] }),
    /invalid_api_response/,
  );
});

test('Driver Match badge counts reject malformed responses without exposing their contents', async () => {
  const route = '/api/dsp/driver-match/counts';
  const counts = { questions: 3 };
  assert.deepEqual(await parseApiResponse(route, 'GET', counts), counts);
  for (const questions of ['private-invalid-count', -1, 0.5, null])
    await assert.rejects(
      parseApiResponse(route, 'GET', { questions }),
      /^Error: invalid_api_response$/,
    );
  await assert.rejects(parseApiResponse(route, 'GET', {}), /^Error: invalid_api_response$/);
});

test('generated platform contracts validate real responses and settings round trips preserve every preference', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: DspSummary) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  const read = async <T>(route: string): Promise<T> => {
    const response = await owner.get(route);
    assert.equal(response.status, 200, response.body);
    const parsed = await parseApiResponse(route, 'GET', response.value);
    assert.deepEqual(parsed, response.value);
    return parsed as T;
  };
  const settingsRoute = '/api/dsp/paycom/settings';
  const [settings, health, dsps] = await Promise.all([
    read<PaycomSettings>(settingsRoute),
    read<PlatformHealth>('/api/platform/health'),
    read<DspSummary[]>('/api/platform/dsps'),
  ]);
  const values: PaycomSettings['values'] = {
    ...settings.values,
    opening_page: 'employees',
    rows_per_page: 25,
    name_order: 'last_first',
    default_sort: 'condition',
    department: 'Operations',
    station: 'DEMO1',
    columns: ['condition', 'totalHours'],
    driver_departments: [],
    late_da_time: '09:45',
    late_da_departments: ['Operations'],
  };
  const saved = await owner.post(settingsRoute, { revision: settings.revision, values });
  assert.equal(saved.status, 200);
  const parsed = (await parseApiResponse(settingsRoute, 'POST', saved.value)) as PaycomSettings;
  assert.deepEqual(parsed.values, values);
  assert.deepEqual(parsed.history[0]!.values, settings.values);
  assert.deepEqual((await read<PaycomSettings>(settingsRoute)).values, values);
  // A later UI edit must preserve the preferences its form does not display.
  const changed = await owner.post(settingsRoute, {
    revision: parsed.revision,
    values: { ...parsed.values, late_da_time: '10:30' },
  });
  assert.equal(changed.status, 200);
  assert.deepEqual(
    ((await parseApiResponse(settingsRoute, 'POST', changed.value)) as PaycomSettings).values,
    { ...values, late_da_time: '10:30' },
  );
  const member = await f.client('member@dispatch.test');
  await member.select(dsp.id);
  const memberSettings = await member.get(settingsRoute);
  assert.deepEqual(
    ((await parseApiResponse(settingsRoute, 'GET', memberSettings.value)) as PaycomSettings)
      .history,
    [],
  );
  const audit = await read<AuditPage>('/api/platform/audit');
  assert(
    audit.events.some(
      (event) => event.action === 'paycom.settings_updated' && event.changes.length,
    ),
  );
  const exported = await owner.post('/api/platform/audit/export', {});
  assert.equal(exported.status, 200);
  assert.deepEqual(
    await parseApiResponse('/api/platform/audit/export', 'POST', exported.value),
    exported.value,
  );

  const invalid = (route: string, body: unknown) =>
    assert.rejects(
      parseApiResponse(route, 'GET', body),
      (error: Error) => error.message === 'invalid_api_response',
    );
  const badHealth = structuredClone(health);
  (badHealth.mail as unknown as Record<string, unknown>).pending = 'two';
  await invalid('/api/platform/health', badHealth);
  const badProfile = structuredClone(dsps);
  (badProfile[0]!.profile as unknown as Record<string, unknown>).removed = 'false';
  await invalid('/api/platform/dsps', badProfile);
  const badAudit = structuredClone(audit);
  (badAudit.events[0]! as unknown as Record<string, unknown>).ref = {
    kind: 'unknown',
    id: 'private-value',
  };
  await invalid('/api/platform/audit', badAudit);
  const badSettings = structuredClone(parsed);
  (badSettings.values as unknown as Record<string, unknown>).columns = ['invented-column'];
  await invalid(settingsRoute, badSettings);
});
