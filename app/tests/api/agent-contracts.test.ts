import test from 'node:test';
import assert from 'node:assert/strict';
import { z } from 'zod';
import { fixture } from '../../../core/shell/tests/support/support.js';

test('REST and compact MCP contracts validate the same answers', async (t) => {
  const f = await fixture({ start: false });
  t.after(f.close);
  const world = JSON.parse(f.cli(['seed-agents']));
  await f.start();
  const owner = await f.client();
  const made = await owner.post('/api/platform/agents/keys', {
    name: 'Response contracts',
    allDsps: false,
    dsps: [world.dsp],
    access: 'read',
    reads: {
      areas: [
        'routes',
        'locations',
        'timecards',
        'meal_breaks',
        'dvic',
        'feedback',
        'safety',
        'returns',
        'scorecard',
      ],
      bypass: false,
    },
    dspReads: [],
    expiresAt: null,
  });
  assert.equal(made.status, 200, made.body);
  const headers = { authorization: `Bearer ${made.value.token}` };
  const server = `http://127.0.0.1:${f.env.PORT}`;
  const get = async (path: string, query: Record<string, string | number | boolean> = {}) => {
    const response = await fetch(
      `${server}${path}?${new URLSearchParams(
        Object.entries(query).map(([key, value]) => [key, String(value)]),
      )}`,
      { headers },
    );
    return { status: response.status, value: await response.json() };
  };
  let id = 0;
  const rpc = async (method: string, params: object = {}) => {
    const response = await fetch(`${server}/api/v1/mcp`, {
      method: 'POST',
      headers: {
        ...headers,
        'content-type': 'application/json',
        accept: 'application/json, text/event-stream',
        'mcp-protocol-version': '2025-06-18',
      },
      body: JSON.stringify({ jsonrpc: '2.0', id: ++id, method, params }),
    });
    assert.equal(response.status, 200);
    const envelope = await response.json();
    assert.equal(envelope.error, undefined, JSON.stringify(envelope));
    return envelope.result;
  };
  const document = (await get('/api/v1/openapi.json')).value;
  const tools = (await rpc('tools/list')).tools;
  const schemas = new Map<string, z.ZodType>();
  for (const [path, item] of Object.entries(document.paths) as [string, any][]) {
    const schema = item.get.responses['200'].content['application/json'].schema;
    assert.ok(Object.keys(schema.properties).length > 0, path);
    schemas.set(path, z.fromJSONSchema(schema));
    for (const status of ['400', '401', '403', '404', '422', '429', '500', '503'])
      assert.ok(
        item.get.responses[status].content['application/json'].schema,
        `${path}: ${status}`,
      );
    for (const status of ['429', '503'])
      assert.equal(item.get.responses[status].headers['Retry-After'].schema.type, 'integer');
  }
  const check = async (
    path: string,
    tool: string,
    query: Record<string, string | number | boolean> = {},
    named: Record<string, string> = {},
  ) => {
    const listed = tools.find((candidate: any) => candidate.name === tool);
    const fullSchema = document.paths[path].get.responses['200'].content['application/json'].schema;
    assert.equal(listed.outputSchema.type, fullSchema.type, tool);
    assert.deepEqual(
      Object.keys(listed.outputSchema.properties).sort(),
      Object.keys(fullSchema.properties).sort(),
      tool,
    );
    assert.deepEqual(listed.outputSchema.required, fullSchema.required, tool);
    const url = Object.entries(named).reduce(
      (url, [key, value]) => url.replace(`{${key}}`, encodeURIComponent(value)),
      path,
    );
    const rest = await get(url, query);
    assert.equal(rest.status, 200, JSON.stringify(rest.value));
    const parsed = schemas.get(path)!.safeParse(rest.value);
    assert.equal(parsed.success, true, `${tool}: ${parsed.error?.message}`);
    const mcp = await rpc('tools/call', { name: tool, arguments: { ...query, ...named } });
    assert.equal(mcp.isError, false, JSON.stringify(mcp));
    const structured = mcp.structuredContent;
    // Discovery summarizes nested records; both transports still satisfy the full contract.
    schemas.get(path)!.parse(structured);
    z.fromJSONSchema(listed.outputSchema).parse(structured);
    assert.deepEqual(JSON.parse(mcp.content[0].text), structured, tool);
    // whoami's timestamp belongs to each separate request.
    assert.deepEqual(
      tool === 'whoami' ? { ...structured, now: rest.value.now } : structured,
      rest.value,
      tool,
    );
    return rest.value;
  };
  await check('/api/v1/whoami', 'whoami');
  await check('/api/v1/status', 'data_status');
  await check('/api/v1/metrics', 'list_metrics');
  const drivers = await check('/api/v1/drivers', 'find_drivers', { include_ids: true, limit: 1 });
  const code = drivers.drivers.rows[0][0];
  await check(
    '/api/v1/drivers/{driver}',
    'driver_report',
    { from: world.from, to: world.to, limit: 1 },
    { driver: code },
  );
  const selected = await check(
    '/api/v1/drivers/{driver}',
    'driver_report',
    {
      from: world.from,
      to: world.to,
      metrics: 'short_inspections,packages_delivered',
      limit: 1,
    },
    { driver: code },
  );
  assert.deepEqual(Object.keys(selected.coverage).sort(), ['dvic', 'routes']);
  await check('/api/v1/team', 'team_table', {
    from: world.from,
    to: world.to,
    metrics: 'packages_delivered,hours_worked,inspections',
    limit: 1,
  });
  await check('/api/v1/team', 'team_table', { date: world.to, per: 'day' });
  const packages = await check('/api/v1/packages', 'packages', {
    date: world.to,
    list: true,
    group_by: 'driver,reason',
    limit: 1,
  });
  const packageContract = z.fromJSONSchema(
    tools.find((tool: any) => tool.name === 'packages').outputSchema,
  );
  // Compaction preserves count/null types, coverage states, table rows and page cursors.
  packageContract.parse({ ...packages, packages: null });
  for (const invalid of [
    { packages: -1 },
    { packages: '0' },
    { coverage: { ...packages.coverage, status: 'unknown' } },
    { coverage: {} },
    { groups: { ...packages.groups, rows: 'not rows' } },
    { groups: { rows: [] } },
    {
      groups: {
        ...packages.groups,
        page: { returned: 1, total: 2, next_cursor: 3 },
      },
    },
    { groups: { ...packages.groups, page: { returned: 1 } } },
  ])
    assert.equal(packageContract.safeParse({ ...packages, ...invalid }).success, false);
  const { understood: _understood, ...withoutInterpretation } = packages;
  assert.equal(packageContract.safeParse(withoutInterpretation).success, false);
  const tracking = packages.list.rows[0][packages.list.columns.indexOf('tracking')];
  await check('/api/v1/packages/{tracking}', 'find_package', {}, { tracking });
  const routes = await check('/api/v1/routes', 'route_day', { date: world.to, limit: 1 });
  await check('/api/v1/routes', 'route_day', {
    from: world.from,
    to: world.to,
    driver: code,
    limit: 1,
  });
  const route = routes.routes.rows[0][routes.routes.columns.indexOf('route')];
  await check('/api/v1/routes/{route}', 'route_stops', { date: world.to }, { route });
  await check(
    '/api/v1/routes/{route}',
    'route_stops',
    { date: world.to, detail: 'full', limit: 1 },
    { route },
  );
  await check('/api/v1/timecards', 'timecards', { date: world.to, limit: 1 });
  await check('/api/v1/timecards', 'timecards', { from: world.from, to: world.to, limit: 1 });
  await check('/api/v1/timecards', 'timecards', {
    driver: code,
    from: world.from,
    to: world.to,
    limit: 1,
  });
  await check('/api/v1/meal-breaks', 'meal_breaks', { date: world.to, issues: true });
  await check('/api/v1/meal-breaks', 'meal_breaks', {
    from: world.from,
    to: world.to,
    driver: code,
    limit: 1,
  });
  await check('/api/v1/dvic', 'dvic_inspections', { date: world.to, limit: 1 });
  await check('/api/v1/dvic', 'dvic_inspections', { date: world.to, detail: 'full', limit: 1 });
  const allInspections = await check('/api/v1/dvic', 'dvic_inspections', {
    from: world.from,
    to: world.to,
    detail: 'full',
    limit: 500,
  });
  assert.equal(allInspections.inspections, allInspections.short);
  assert.ok(allInspections.list.rows.every((row: any[]) => row[6] === true));
  const inspectionPeople = await check('/api/v1/drivers', 'find_drivers', {
    include_ids: true,
    limit: 500,
  });
  const inspectionDriver = inspectionPeople.drivers.rows.find((person: any[]) =>
    allInspections.list.rows.some(
      (row: any[]) => row[1] === person[1] || row[1] === `${person[1]} (${person[0]})`,
    ),
  );
  assert.ok(inspectionDriver, 'The fixture must contain a matched driver with inspections');
  const driverInspections = await check('/api/v1/dvic', 'dvic_inspections', {
    driver: inspectionDriver[0],
    from: world.from,
    to: world.to,
    detail: 'full',
    limit: 500,
  });
  assert.ok(driverInspections.inspections > 0);
  assert.deepEqual(
    driverInspections.list.rows,
    allInspections.list.rows.filter((row: any[]) => row[1] === driverInspections.understood.driver),
  );
  assert.equal(driverInspections.inspections, driverInspections.list.rows.length);
  assert.deepEqual(driverInspections.coverage, allInspections.coverage);
  for (const [path, tool] of [
    ['/api/v1/feedback', 'customer_feedback'],
    ['/api/v1/safety', 'safety_events'],
    ['/api/v1/returns', 'returns'],
  ]) {
    const both = await check(path!, tool!, {
      from: world.from,
      to: world.to,
      group_by: 'driver,week',
      list: true,
      limit: 1,
    });
    assert.ok(both.list.page.next_cursor);
    const next = await check(path!, tool!, {
      from: world.from,
      to: world.to,
      group_by: 'driver,week',
      list: true,
      limit: 1,
      cursor: both.list.page.next_cursor,
      groups_cursor: both.groups.page.next_cursor,
    });
    assert.notDeepEqual(next.list.rows, both.list.rows);
    assert.notDeepEqual(next.groups.rows, both.groups.rows);
    await check(path!, tool!, { from: world.from, to: world.to, list: true, limit: 1 });
  }
  const disputed = await check('/api/v1/safety', 'safety_events', {
    from: world.from,
    to: world.to,
    counting: false,
    list: true,
  });
  assert.equal(disputed.counting, 0);
  const businessClosed = await check('/api/v1/returns', 'returns', {
    from: world.from,
    to: world.to,
    reason: 'business_closed',
    impacting: true,
    list: true,
  });
  assert.ok(
    businessClosed.list.rows.every((row: any[]) => row[3] === 'business_closed' && row[5] === true),
  );
  await check('/api/v1/scorecard', 'scorecard', { limit: 1 });
  await check('/api/v1/scorecard', 'scorecard', { week: '2099-W01' });
  // Uncollected days still have a valid, explicit response contract.
  for (const [path, tool] of [
    ['/api/v1/routes', 'route_day'],
    ['/api/v1/packages', 'packages'],
    ['/api/v1/timecards', 'timecards'],
    ['/api/v1/meal-breaks', 'meal_breaks'],
    ['/api/v1/dvic', 'dvic_inspections'],
  ]) {
    const empty = await check(path!, tool!, { date: '2099-01-01' });
    assert.equal(empty.coverage.status, 'missing', tool);
  }
  const refusal = await get('/api/v1/packages', { limit: 0 });
  assert.equal(refusal.status, 400);
  z.fromJSONSchema(
    document.paths['/api/v1/packages'].get.responses['400'].content['application/json'].schema,
  ).parse(refusal.value);
});
