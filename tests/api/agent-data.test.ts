import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../support/support.js';
import type { AgentKeyCreated, AgentKeys } from '../../shared/contracts/index.js';

test('an agent reads a DSP by asking in its own words, and is told what to fix', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const { dsps }: AgentKeys = (await owner.get('/api/platform/agents')).value;
  const north = dsps.find((dsp) => dsp.name === 'Northline Logistics')!;
  const key = async (name: string, reach: string[]) => {
    const made = await owner.post('/api/platform/agents/keys', {
      name,
      allDsps: reach.length === 0,
      dsps: reach,
      access: 'read',
      tools: 'full',
      locations: false,
      expiresAt: null,
    });
    assert.equal(made.status, 200, made.body);
    const { token }: AgentKeyCreated = made.value;
    return (path: string) => f.request(path, undefined, { authorization: `Bearer ${token}` });
  };
  const one = await key('Northline only', [north.id]);
  const every = await key('Every DSP', []);

  // The description an agent or its harness loads, every operation behind the key.
  const spec = await one('/api/v1/openapi.json');
  assert.equal(spec.status, 200, spec.body);
  assert.equal(spec.value.openapi, '3.1.0');
  assert.equal(spec.value.components.securitySchemes.key.scheme, 'bearer');
  const paths = Object.keys(spec.value.paths);
  for (const path of ['/api/v1/team', '/api/v1/drivers/{driver}', '/api/v1/meal-breaks'])
    assert.ok(paths.includes(path), path);
  for (const operation of Object.values(spec.value.paths) as { get: { operationId: string } }[])
    assert.ok(operation.get.operationId);
  const metrics = (await one('/api/v1/metrics')).value.metrics as { name: string }[];
  assert.ok(metrics.some((metric) => metric.name === 'hours_worked'));

  // A key for one DSP never has to name it; a person is found by how they are called.
  const listed = await one('/api/v1/drivers');
  assert.equal(listed.status, 200, listed.body);
  assert.equal(listed.value.dsp.name, 'Northline Logistics');
  const someone = listed.value.drivers[0] as { code: string; name: string };
  const report = await one(`/api/v1/drivers/${encodeURIComponent(someone.name)}`);
  assert.equal(report.status, 200, report.body);
  assert.equal(report.value.driver.code, someone.code);
  assert.ok(report.value.period.from <= report.value.period.to);
  assert.equal(report.value.coverage.timecards.enabled, true);
  const team = await one('/api/v1/team?period=last%207%20days&metrics=hours_worked');
  assert.equal(team.status, 200, team.body);
  assert.equal(team.value.period.days, 7);

  // What is unclear is refused with the choices, never guessed.
  const several = await every('/api/v1/drivers');
  assert.deepEqual([several.status, several.value.error], [400, 'dsp_required']);
  assert.ok(several.value.choices.includes('Northline Logistics'));
  assert.equal(
    (await every('/api/v1/drivers?dsp=northline')).value.dsp.name,
    'Northline Logistics',
  );
  const misspelt = await one('/api/v1/team?metric=hours_worked');
  assert.deepEqual([misspelt.status, misspelt.value.error], [400, 'unknown_parameter']);
  assert.ok(misspelt.value.choices.includes('metrics'));
  assert.match(misspelt.value.message, /metric/);
  const unknown = await one('/api/v1/team?metrics=steps');
  assert.deepEqual([unknown.status, unknown.value.error], [400, 'unknown_metric']);
  const nobody = await one('/api/v1/drivers/Nobody%20Anywhere');
  assert.deepEqual([nobody.status, nobody.value.error], [404, 'driver_not_found']);
});
