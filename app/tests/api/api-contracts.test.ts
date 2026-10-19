import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../core/shell/tests/support/support.js';
import '../support/manifests.js';
import { parseApiResponse } from '../../../core/shell/frontend/runtime/replies.js';
import type { AuditPage, PlatformHealth } from '../../../core/platform_owner/api/index.js';
import type { DspSummary } from '../../../core/accounts/api/index.js';
import type { SettingsAnswer } from '../../../core/settings/api/index.js';
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

test('generated platform contracts validate real responses', async (t) => {
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
  // Settings saves what the page reads, and the page reads it as saved.
  const { sections } = await read<SettingsAnswer>('/api/dsp/settings');
  const roster = sections.find((section) => section.id === 'timecard.roster')!;
  const saved = await owner.post(`/api/dsp/settings/${roster.id}`, {
    values: { name_order: 'last_first', department: 'Operations' },
    revision: roster.revision,
  });
  assert.equal(saved.status, 200, saved.body);
  const parsed = await read<PaycomSettings>(settingsRoute);
  assert.deepEqual(parsed, { ...settings, nameOrder: 'last_first' });
  const member = await f.client('member@dispatch.test');
  await member.select(dsp.id);
  assert.deepEqual((await member.get(settingsRoute)).value, parsed);
  const audit = await read<AuditPage>('/api/platform/audit');
  assert(audit.events.some((event) => event.action === 'settings.changed' && event.changes.length));
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
  (badSettings as unknown as Record<string, unknown>).nameOrder = 'invented-order';
  await invalid(settingsRoute, badSettings);
});
