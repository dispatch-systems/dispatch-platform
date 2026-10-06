// Benchmark a private synthetic DSP with full responses, authentication and normal quotas.
// npm run benchmark:agents -- --binary target/release/dispatch-backend --output report.json
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { parseArgs } from 'node:util';
import { fixture } from '../testing/fixture-server.js';

const { values } = parseArgs({
  options: {
    binary: { type: 'string', default: 'target/release/dispatch-backend' },
    output: { type: 'string' },
  },
});
const binary = path.resolve(values.binary!);
const f = await fixture({ binary, start: false });
type Workload = {
  name: string;
  path: string;
  tool: string;
  args: Record<string, string | number | boolean>;
};
type Transport = 'rest' | 'mcp-2025-06-18' | 'mcp-2026-07-28';
try {
  const world = JSON.parse(f.cli(['seed-agents']));
  await f.start();
  const owner = await f.client();
  const origin = `http://127.0.0.1:${f.env.PORT}`;
  const period = { from: world.from as string, to: world.to as string };
  const workloads: Workload[] = [
    { name: 'package-count', path: '/api/v1/packages', tool: 'packages', args: period },
    {
      name: 'package-groups',
      path: '/api/v1/packages',
      tool: 'packages',
      args: { ...period, group_by: 'driver,reason' },
    },
    {
      name: 'package-page',
      path: '/api/v1/packages',
      tool: 'packages',
      args: { ...period, list: true, limit: 100 },
    },
    {
      name: 'team-period',
      path: '/api/v1/team',
      tool: 'team_table',
      args: { ...period, metrics: 'packages_delivered,hours_worked,inspections', limit: 100 },
    },
    {
      name: 'timecards-day',
      path: '/api/v1/timecards',
      tool: 'timecards',
      args: { date: world.to },
    },
    {
      name: 'dvic-page',
      path: '/api/v1/dvic',
      tool: 'dvic_inspections',
      args: { ...period, detail: 'full', limit: 100 },
    },
    {
      name: 'feedback-groups',
      path: '/api/v1/feedback',
      tool: 'customer_feedback',
      args: { ...period, group_by: 'driver,week' },
    },
    {
      name: 'scorecard-page',
      path: '/api/v1/weekly-scorecard',
      tool: 'weekly_scorecard',
      args: { limit: 100 },
    },
    {
      name: 'route-range',
      path: '/api/v1/routes',
      tool: 'route_day',
      args: { ...period, limit: 100 },
    },
    {
      name: 'meal-range',
      path: '/api/v1/meal-breaks',
      tool: 'meal_breaks',
      args: { ...period, limit: 100 },
    },
    {
      name: 'safety-page',
      path: '/api/v1/safety',
      tool: 'safety_events',
      args: { ...period, list: true, limit: 100 },
    },
    {
      name: 'returns-groups',
      path: '/api/v1/returns',
      tool: 'returns',
      args: { ...period, contact: 'missed', group_by: 'driver', list: true, limit: 100 },
    },
    {
      name: 'driver-metrics',
      path: '/api/v1/drivers/Taylor%20Brooks',
      tool: 'driver_report',
      args: {
        ...period,
        driver: 'Taylor Brooks',
        metrics: 'short_inspections,packages_delivered',
        limit: 100,
      },
    },
  ];
  const key = async (name: string) => {
    const made = await owner.post('/api/platform/agents/keys', {
      name,
      allDsps: false,
      dsps: [world.dsp],
      access: 'read',
      reads: {
        areas: [
          'routes',
          'timecards',
          'meal_breaks',
          'dvic',
          'feedback',
          'safety',
          'returns',
          'weekly_scorecard',
        ],
        bypass: false,
      },
      dspReads: [],
      expiresAt: null,
    });
    assert.equal(made.status, 200, made.body);
    return made.value.token as string;
  };
  let id = 0;
  const request = async (transport: Transport, token: string, work: Workload) => {
    const headers: Record<string, string> = { authorization: `Bearer ${token}` };
    let url = origin + work.path;
    let init: RequestInit = { headers, signal: AbortSignal.timeout(15_000) };
    if (transport === 'rest') {
      url +=
        '?' +
        new URLSearchParams(
          Object.entries(work.args)
            .filter(([key]) => key !== 'driver' || work.tool !== 'driver_report')
            .map(([key, value]) => [key, String(value)]),
        );
    } else {
      const version = transport.slice(4);
      const modern = version === '2026-07-28';
      Object.assign(headers, {
        'content-type': 'application/json',
        accept: 'application/json, text/event-stream',
        'mcp-protocol-version': version,
      });
      if (modern) Object.assign(headers, { 'mcp-method': 'tools/call', 'mcp-name': work.tool });
      url = origin + '/api/v1/mcp';
      init = {
        ...init,
        method: 'POST',
        body: JSON.stringify({
          jsonrpc: '2.0',
          id: ++id,
          method: 'tools/call',
          params: {
            name: work.tool,
            arguments: work.args,
            ...(modern
              ? {
                  _meta: {
                    'io.modelcontextprotocol/protocolVersion': version,
                    'io.modelcontextprotocol/clientInfo': {
                      name: 'dispatch-benchmark',
                      version: '1',
                    },
                    'io.modelcontextprotocol/clientCapabilities': {},
                  },
                }
              : {}),
          },
        }),
      };
    }
    const started = performance.now();
    const response = await fetch(url, init);
    const body = await response.text();
    const ms = performance.now() - started;
    assert.equal(response.status, 200, `${transport}/${work.name}: ${body}`);
    const value = JSON.parse(body);
    if (transport !== 'rest') {
      assert.equal(value.error, undefined, body);
      assert.equal(value.result.isError, false, body);
      assert.deepEqual(JSON.parse(value.result.content[0].text), value.result.structuredContent);
    }
    return {
      ms,
      bytes: Buffer.byteLength(body),
      answer: transport === 'rest' ? value : value.result.structuredContent,
    };
  };
  // Validate parity before measuring. This key is separate from those whose quota we measure.
  const canonicalKey = await key('Benchmark correctness');
  const discoveryHeaders = { authorization: `Bearer ${canonicalKey}` };
  const [openapi, skill, catalog] = await Promise.all([
    fetch(origin + '/api/v1/openapi.json', { headers: discoveryHeaders }).then((response) =>
      response.text(),
    ),
    fetch(origin + '/api/v1/skill', { headers: discoveryHeaders }).then((response) =>
      response.text(),
    ),
    fetch(origin + '/api/v1/mcp', {
      method: 'POST',
      headers: {
        ...discoveryHeaders,
        'content-type': 'application/json',
        accept: 'application/json, text/event-stream',
        'mcp-protocol-version': '2025-06-18',
      },
      body: JSON.stringify({ jsonrpc: '2.0', id: ++id, method: 'tools/list', params: {} }),
    }).then((response) => response.text()),
  ]);
  const expected = await Promise.all(
    workloads.map(async (work) => (await request('rest', canonicalKey, work)).answer),
  );
  assert.ok(expected[0].packages > 0, 'The fixture must contain packages to count');
  assert.ok(expected[3].rows.rows.length > 0, 'The fixture must contain a team');
  assert.ok(expected[5].list.rows.length > 0, 'The fixture must contain DVIC inspections');
  assert.ok(expected[6].feedback > 0, 'The fixture must contain feedback');
  assert.equal(expected[7].posted, true, 'The fixture must contain a posted scorecard');
  const measurements = [];
  for (const transport of ['rest', 'mcp-2025-06-18', 'mcp-2026-07-28'] as const) {
    for (const concurrency of [1, 4, 8]) {
      // Each pass has its own key so additional workloads stay below 120/minute.
      for (const pass of ['first', 'repeat']) {
        const token = await key(`Benchmark ${transport} c${concurrency} ${pass}`);
        const samples: { workload: number; ms: number; bytes: number }[] = [];
        let next = 0;
        const count = workloads.length * 6;
        const began = performance.now();
        await Promise.all(
          Array.from({ length: concurrency }, async () => {
            while (next < count) {
              const index = next++ % workloads.length;
              const sample = await request(transport, token, workloads[index]!);
              assert.deepEqual(
                sample.answer,
                expected[index],
                `${transport}/${workloads[index]!.name}`,
              );
              samples.push({ workload: index, ms: sample.ms, bytes: sample.bytes });
            }
          }),
        );
        const elapsedMs = performance.now() - began;
        const summary = (selected: typeof samples) => {
          const times = selected.map((sample) => sample.ms).sort((a, b) => a - b);
          return {
            requests: selected.length,
            medianMs: +times[Math.floor(times.length / 2)]!.toFixed(2),
            p95Ms: +times[Math.ceil(times.length * 0.95) - 1]!.toFixed(2),
            meanResponseBytes: Math.round(
              selected.reduce((sum, sample) => sum + sample.bytes, 0) / selected.length,
            ),
          };
        };
        measurements.push({
          transport,
          concurrency,
          pass,
          ...summary(samples),
          requestsPerSecond: +((count * 1000) / elapsedMs).toFixed(2),
          workloads: workloads.map((work, index) => ({
            name: work.name,
            ...summary(samples.filter((sample) => sample.workload === index)),
          })),
        });
      }
    }
  }
  const report = {
    format: 1,
    binarySha256: createHash('sha256').update(fs.readFileSync(binary)).digest('hex'),
    machine: {
      platform: os.platform(),
      architecture: os.arch(),
      cpu: os.cpus()[0]?.model,
      logicalCpus: os.cpus().length,
    },
    dataset: {
      from: world.from,
      to: world.to,
      packages: expected[0].packages,
      teamRows: expected[3].rows.rows.length,
    },
    discoveryBytes: {
      openapi: Buffer.byteLength(openapi),
      skill: Buffer.byteLength(skill),
      tools: Buffer.byteLength(catalog),
    },
    failures: 0,
    measurements,
    note: 'Synthetic fixture on loopback. First/repeat passes are not cold-cache measurements. Six samples per workload; use repeated runs for comparisons. All responses checked against the REST baseline. Normal quotas remain enabled.',
  };
  const output = JSON.stringify(report, null, 2) + '\n';
  if (values.output) fs.writeFileSync(path.resolve(values.output), output);
  process.stdout.write(output);
} finally {
  await f.close();
}
