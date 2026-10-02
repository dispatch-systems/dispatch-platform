import test, { type TestContext } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import type { AddressInfo } from 'node:net';
import { fixture, until } from '../support/support.js';
import { paycomFixture, credentials } from '../support/browseros-paycom-fixture.js';
import { executionPage, itineraryApi, type Quirks } from '../support/cortex-execution.js';

const native = process.env.DISPATCH_TEST_NATIVE !== '1';
// An execution page's list props for `date`'s routes, each driver named for its route.
const listProps = (date: string, routes: { transporterId: string; routeCode: string }[]) => ({
  selectedDay: date,
  serviceAreaId: 'area-1',
  selectedStation: {
    serviceAreaID: 'area-1',
    defaultStationCode: 'TST1',
    timeZone: 'US/Pacific',
  },
  providerFilterValue: 'provider-1',
  providerFilterOptions: [{ value: 'provider-1' }],
  isLoadingSummaries: false,
  allItinerarySummaries: routes,
  transporterSummary: Object.fromEntries(
    routes.map((r) => [r.transporterId, { transporterName: `Driver ${r.routeCode}` }]),
  ),
});
const delivered = (id: string, time: number) => ({
  taskId: id,
  taskType: 'DROP_OFF',
  taskState: 'DELIVERED',
  executionStatus: 'COMPLETE',
  actualExecutionTime: time / 1000,
  transporterId: null,
});
type Answer = (url: URL) => Promise<{ json: unknown } | { html: string } | { status: number }>;
// A Cortex site answered by `answer`, and a DSP whose Cortex connection signs in to it.
async function site(t: TestContext, answer: Answer) {
  const server = http.createServer(async (req, res) => {
    const url = new URL(req.url!, 'http://fixture.test');
    if (url.pathname === '/dspconsolev2') {
      res.setHeader('content-type', 'text/html');
      res.end(
        '<title>DSP Console</title><nav><a href="/scheduling/calendar-view/week">Weekly schedule</a></nav><a href="/ap/signin">Sign out</a>',
      );
      return;
    }
    if (
      !url.pathname.startsWith(itineraryApi) &&
      !url.pathname.startsWith('/operations/execution/itineraries')
    ) {
      res.writeHead(404);
      res.end();
      return;
    }
    const answered = await answer(url);
    if ('status' in answered) {
      res.writeHead(answered.status);
      res.end();
    } else if ('json' in answered) {
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify(answered.json));
    } else {
      res.setHeader('content-type', 'text/html');
      res.end(answered.html);
    }
  });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  t.after(() => {
    server.closeAllConnections();
    server.close();
  });
  const f = await fixture({
    env: {
      DISPATCH_FIXTURE_PROVIDER_URL: `http://fixture.dispatch.invalid:${(server.address() as AddressInfo).port}`,
      DISPATCH_BWRAP_EXECUTABLE:
        process.env.DISPATCH_BWRAP_EXECUTABLE ?? '/usr/local/libexec/dispatch-dev/bwrap',
    },
  });
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: any) => d.permanent);
  await owner.select(dsp.id);
  const saved = await owner.post('/api/dsp/connections/cortex', {
    username: 'fixture@example.test',
    password: 'fixture-password',
  });
  assert.equal(saved.value.status, 'ready', saved.body);
  // Collects `date`'s meals and waits for the job to end.
  const collect = async (date: string, requestId: string, wait = 70000) => {
    const response = await owner.post('/api/dsp/cortex/meal-breaks/collect', {
      date,
      station: 'TST1',
      serviceAreaId: 'area-1',
      provider: 'provider-1',
      timezone: 'America/Los_Angeles',
      requestId,
    });
    assert.equal(response.status, 202, response.body);
    let job: any;
    await until(async () => {
      job = (await owner.read('/api/dsp/jobs')).find((j: any) => j.id === response.value.id);
      return ['succeeded', 'failed'].includes(job.status);
    }, wait);
    return job;
  };
  return { f, owner, dsp, collect };
}

test(
  'first Sync Now discovers Cortex scope and publishes both sources through BrowserOS',
  { skip: native, timeout: 120000 },
  async (t) => {
    const date = '2026-09-06';
    const start = Date.parse(`${date}T20:00:00Z`);
    const paths: string[] = [];
    const candidate = {
      itineraryId: 'itinerary-1',
      transporterId: 'driver-1',
      routeCode: 'CX1',
      companyId: 'provider-1',
      executionStatus: 'COMPLETE',
      stopProgress: { total: 2, completed: 2 },
      latestTaskExecutionTime: start + 2400000,
      breaks: [
        {
          punchId: 'punch-1',
          breakId: 'meal-1',
          type: 'MEAL',
          state: 'OFF',
          timeStampOn: start,
          timeStampOff: start + 1800000,
          sequenceNumber: 1,
        },
      ],
    };
    const f = await paycomFixture((req, res) => {
      const url = new URL(req.url!, 'http://fixture.test');
      if (url.pathname === '/dspconsolev2') {
        res.end(
          '<title>DSP Console</title><nav><a href="/scheduling/calendar-view/week">Weekly schedule</a></nav><a href="/ap/signin">Sign out</a>',
        );
        return true;
      }
      // Like Cortex, send an itineraries request without a service area to the
      // execution home, whose station list lives in hook state rather than props.
      if (url.pathname.startsWith(itineraryApi)) {
        assert.equal(url.searchParams.get('itineraryId'), 'itinerary-1');
        res.setHeader('Content-Type', 'application/json');
        res.end(
          JSON.stringify({
            itineraryDetails: {
              ...candidate,
              localDate: [2026, 9, 6],
              serviceAreaId: 'area-1',
              unknownStops: [],
              inactiveTasks: [],
              stops: [start - 60000, start + 1860000].map((time, i) => ({
                tasks: [
                  {
                    taskId: `task-${i}`,
                    taskType: 'DROP_OFF',
                    taskState: 'DELIVERED',
                    executionStatus: 'COMPLETE',
                    actualExecutionTime: time / 1000,
                    transporterId: null,
                  },
                ],
              })),
            },
          }),
        );
        return true;
      }
      if (url.pathname === '/operations/execution/') {
        const areas = {
          'area-2': { serviceAreaID: 'area-2', defaultStationCode: 'ABC1', timeZone: 'US/Pacific' },
          'area-1': { serviceAreaID: 'area-1', defaultStationCode: 'TST1', timeZone: 'US/Pacific' },
        };
        res.setHeader('Content-Type', 'text/html');
        res.end(
          `<title>Delivery Execution</title><main></main><script>document.querySelector('main').__reactFiber$fixture={memoizedProps:{},memoizedState:{memoizedState:document.body,next:{memoizedState:{current:${JSON.stringify(areas)}},next:null}}};</script>`,
        );
        return true;
      }
      if (!url.pathname.startsWith('/operations/execution/itineraries')) return false;
      paths.push(url.pathname + url.search);
      if (!url.searchParams.get('serviceAreaId')) {
        res.writeHead(302, { Location: '/operations/execution/' });
        res.end();
        return true;
      }
      const p: any = {
        selectedDay: date,
        serviceAreaId: 'area-1',
        selectedStation: {
          serviceAreaID: 'area-1',
          defaultStationCode: 'TST1',
          timeZone: 'US/Pacific',
        },
        providerFilterValue: url.searchParams.get('provider') ?? 'ALL_DSPS',
        providerFilterOptions: [
          { value: 'ALL_DRIVERS', label: 'All Drivers' },
          { value: 'ALL_DSPS' },
          { value: 'provider-1', label: 'NLOG' },
          { value: 'other-provider', label: 'Other' },
        ],
        isLoadingSummaries: false,
        allItinerarySummaries: [candidate],
        transporterSummary: { 'driver-1': { transporterName: 'Fixture Driver' } },
      };
      res.setHeader('Content-Type', 'text/html');
      res.end(executionPage(p));
      return true;
    });
    t.after(f.close);
    const owner = await f.client();
    const dsp = owner.session.dsps.find((d: any) => d.name === 'Northline Logistics');
    await owner.select(dsp.id);
    assert.equal(
      (
        await owner.post('/api/dsp/profile', {
          name: 'Northline Logistics',
          abbreviation: 'NLOG',
          stationCode: 'TST1',
          timezone: 'America/Los_Angeles',
        })
      ).status,
      200,
    );
    await owner.select(dsp.id);
    for (const [provider, login] of [
      ['paycom', credentials],
      ['cortex', { username: 'fixture@example.test', password: 'fixture-password' }],
    ] as const) {
      const saved = await owner.post(`/api/dsp/connections/${provider}`, login);
      assert.equal(saved.value.status, 'ready', saved.body);
    }
    assert.deepEqual((await owner.get(`/api/dsp/cortex/meal-breaks?date=${date}`)).value, []);
    const request = { requestId: 'first-sync', date };
    const queued = await owner.post('/api/dsp/jobs/meal-breaks', request);
    assert.equal(queued.status, 202, queued.body);
    assert.equal(queued.value.jobs.length, 2);
    await until(async () => {
      const jobs = (await owner.read('/api/dsp/jobs')).filter((j: any) =>
        queued.value.jobs.some((q: any) => q.id === j.id),
      );
      assert(!jobs.some((j: any) => j.status === 'failed'), JSON.stringify(jobs));
      return jobs.every((j: any) => j.status === 'succeeded');
    }, 90000);
    const status = (await owner.get(`/api/dsp/jobs/meal-breaks?date=${date}`)).value;
    assert(status.paycom.collectedAt);
    assert(status.flex.collectedAt);
    const publications = (await owner.get(`/api/dsp/cortex/meal-breaks?date=${date}`)).value;
    assert.equal(publications[0].mealCount, 1);
    assert.equal(publications[0].verifiedGapPairs, 1);
    assert(
      paths.some((path) => !path.includes('serviceAreaId=')),
      'Starts without a known station ID',
    );
    assert(
      paths.some(
        (path) => path.includes('serviceAreaId=area-1') && path.includes('provider=provider-1'),
      ),
      'Collects the resolved DSP scope',
    );
    assert.deepEqual(
      (await owner.post('/api/dsp/jobs/meal-breaks', request)).value.jobs.map((j: any) => j.id),
      queued.value.jobs.map((j: any) => j.id),
    );
  },
);
test(
  'Cortex BrowserOS captures changed meals, multiple itineraries, and four meal timestamps before atomic publication',
  { skip: native, timeout: 180000 },
  async (t) => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    t.after(() => release());
    let lists = 0;
    let mode = 'growing';
    const visits: Record<string, number> = {};
    const start = Date.parse('2026-01-10T22:34:00Z');
    const br = (id: string, on: number, off: number | null) => ({
      punchId: id,
      breakId: 'break-' + id,
      type: 'MEAL',
      state: off === null ? 'ON' : 'OFF',
      timeStampOn: on,
      timeStampOff: off,
      sequenceNumber: 1,
    });
    const summaries = () => {
      const grown = mode !== 'growing' || lists >= 2;
      return [
        {
          itineraryId: 'itinerary-1',
          transporterId: 'driver-1',
          routeCode: 'CX1',
          companyId: 'provider-1',
          executionStatus: 'COMPLETE',
          // Deliveries keep advancing during a working day; only meal and
          // route facts may restart a read.
          stopProgress: { total: 2, completed: lists },
          latestTaskExecutionTime: start + 2400000 + lists,
          lastStopExecutionTime: start + 2400000 + lists,
          breaks: grown
            ? [
                br('meal#1', start, start + 900000),
                br('meal#2', start + 3600000, start + 4500000),
                { ...br('start-punch', start + 600, null), breakId: 'break-meal#1' },
              ]
            : [],
        },
        {
          itineraryId: 'itinerary-2',
          transporterId: 'driver-1',
          routeCode: 'CX2',
          companyId: 'provider-1',
          executionStatus: 'COMPLETE',
          stopProgress: { total: 2, completed: 2 },
          latestTaskExecutionTime: start + 2400000,
          breaks: [],
        },
      ];
    };
    const { f, owner, dsp, collect } = await site(t, async (url) => {
      if (url.pathname.startsWith(itineraryApi)) {
        const id = url.searchParams.get('itineraryId')!;
        visits[id] = (visits[id] || 0) + 1;
        const c = summaries().find((s) => s.itineraryId === id)!;
        const details: any = {
          ...c,
          breaks: [...c.breaks].reverse(),
          localDate: [2026, 1, 10],
          serviceAreaId: 'area-1',
          stops: [
            { tasks: [delivered('task.0', start - 240000), delivered('task.1', start - 60000)] },
            {
              tasks: [
                delivered('task.1', start - (mode === 'conflicting' ? 30000 : 60000)),
                delivered('task.2', start + 960000),
                delivered('task.3', start + 1080000),
                delivered('task.4', start + 4560000),
              ],
            },
          ],
          // Amazon's unplanned dwell locations are separate from delivery tasks.
          unknownStops: [
            {
              unknownStopId: 'unknown-1',
              enterTime: start / 1000,
              exitTime: (start + 900000) / 1000,
              previousPlannedStopNumber: 1,
            },
          ],
          inactiveTasks: [
            delivered('removed-task', start - (mode === 'inactive-near' ? 30000 : 7200000)),
          ],
          stopProgress: { total: mode === 'unavailable' ? 3 : 2 },
        };
        if (mode === 'invalid') details.breaks = [br('bad', start, start - 1000)];
        if (mode === 'meal-conflict' && id === 'itinerary-1')
          details.breaks.push({
            ...br('conflicting-punch', start, start + 1900000),
            breakId: 'break-meal#1',
          });
        return { json: { itineraryDetails: details } };
      }
      if (!url.pathname.includes('/documentType/')) {
        lists++;
        if (lists === 3) await gate;
      }
      const p = listProps('2026-01-10', summaries());
      p.transporterSummary = { 'driver-1': { transporterName: 'Fixture Driver' } };
      return { html: executionPage(p) };
    });
    const first = collect('2026-01-10', 'first');
    await until(async () => {
      const comparison = await owner.read('/api/dsp/paycom/meal-breaks?date=2026-01-10');
      return comparison.rows.some((row: any) => row.cortex.length === 2);
    }, 40000);
    assert.equal(
      (await owner.get('/api/dsp/cortex/meal-breaks?date=2026-01-10')).value.length,
      0,
      'The complete snapshot is still unpublished',
    );
    assert(
      (await owner.get('/api/dsp/jobs')).value.some(
        (job: any) => job.kind === 'cortex.meal_breaks.collect' && job.status === 'running',
      ),
    );
    release();
    assert.equal((await first).status, 'succeeded');
    assert(visits['itinerary-1']! >= 2, 'Changed existing meals must be re-read');
    // Read from its own response, whatever the page shows, a route needs no reload.
    assert.equal(visits['itinerary-2'], 1, 'An unchanged route is read once');
    const publications = () => owner.get('/api/dsp/cortex/meal-breaks?date=2026-01-10');
    const initial = (await publications()).value;
    assert.equal(initial[0].itineraryCount, 2);
    assert.equal(initial[0].mealCount, 2);
    assert.equal(initial[0].verifiedGapPairs, 2);
    f.database(`dsps/${dsp.id}/data/cortex/cortex.sqlite`, (db) => {
      const rows = db
        .prepare(
          'SELECT last_delivery_at,started_at,ended_at,first_delivery_at FROM meal_records ORDER BY meal_id',
        )
        .all();
      assert.deepEqual(
        rows.map((r) => ({ ...r })),
        [
          {
            last_delivery_at: '2026-01-10T22:33:00.000Z',
            started_at: '2026-01-10T22:34:00.000Z',
            ended_at: '2026-01-10T22:49:00.000Z',
            first_delivery_at: '2026-01-10T22:50:00.000Z',
          },
          {
            last_delivery_at: '2026-01-10T22:52:00.000Z',
            started_at: '2026-01-10T23:34:00.000Z',
            ended_at: '2026-01-10T23:49:00.000Z',
            first_delivery_at: '2026-01-10T23:50:00.000Z',
          },
        ],
      );
      assert.equal((db.prepare('SELECT count(*) n FROM meal_delivery_events').get() as any).n, 0);
      assert.equal((db.prepare('SELECT count(*) n FROM meal_breaks').get() as any).n, 0);
      const links = db
        .prepare('SELECT itinerary_id id,url FROM meal_sources ORDER BY itinerary_id')
        .all() as { id: string; url: string }[];
      assert.deepEqual(
        links.map((l) => l.id),
        ['itinerary-1', 'itinerary-2'],
      );
      for (const { id, url } of links) {
        const link = new URL(url);
        assert.equal(
          link.pathname,
          `/operations/execution/itineraries/${id}/documentType/Itinerary`,
        );
        assert.equal(link.searchParams.get('selectedDay'), '2026-01-10');
      }
    });
    mode = 'invalid';
    const failed = await collect('2026-01-10', 'bad');
    assert.equal(failed.status, 'failed');
    assert.equal(failed.error, 'cortex_invalid_meal_evidence');
    assert.deepEqual((await publications()).value, initial);
    mode = 'meal-conflict';
    const mealConflict = await collect('2026-01-10', 'meal-conflict');
    assert.equal(mealConflict.status, 'failed');
    assert.equal(mealConflict.error, 'cortex_invalid_meal_evidence');
    assert.deepEqual((await publications()).value, initial);
    mode = 'unavailable';
    assert.equal((await collect('2026-01-10', 'unknown')).status, 'succeeded');
    assert.equal((await publications()).value[0].verifiedGapPairs, 0);
    mode = 'inactive-near';
    assert.equal((await collect('2026-01-10', 'inactive-near')).status, 'succeeded');
    assert.equal((await publications()).value[0].verifiedGapPairs, 0);
    mode = 'conflicting';
    assert.equal((await collect('2026-01-10', 'conflict')).status, 'succeeded');
    const conflict = (await publications()).value[0];
    assert.equal(conflict.mealCount, 2);
    assert.equal(conflict.verifiedGapPairs, 0);
    f.database(`dsps/${dsp.id}/data/cortex/cortex.sqlite`, (db) => {
      assert.equal(
        (
          db
            .prepare(
              "SELECT count(*) n FROM meal_delivery_events WHERE publication_id=? AND event_id='task.1'",
            )
            .get(conflict.id) as any
        ).n,
        0,
      );
      assert.equal(
        (
          db
            .prepare(
              "SELECT count(*) n FROM meal_itineraries WHERE publication_id=? AND delivery_coverage='unavailable'",
            )
            .get(conflict.id) as any
        ).n,
        2,
      );
    });
  },
);
test(
  'Cortex reads every route of a day three at once, however many routes the day has',
  { skip: native, timeout: 240000 },
  async (t) => {
    // Days of one, two and 25 routes. On the busy day a rescue route joins after the
    // first list is read, and a driver ends a meal while the day is read.
    const sizes: Record<string, number> = { '2026-01-12': 1, '2026-01-13': 2, '2026-01-14': 25 };
    const busy = '2026-01-14';
    const swipe = `itinerary-${busy}-7`;
    const lists: Record<string, number> = {};
    // Itinerary responses by route, route pages loaded by day, and responses being
    // answered at once, most of all by day.
    const reads: Record<string, number> = {};
    const pages: Record<string, number> = {};
    const peaks: Record<string, number> = {};
    // Responses held until every tab has asked: how soon a tab starts is the machine's
    // pace, not the collector's, so the peak never rests on a fixed delay.
    const held: Record<string, (() => void)[]> = {};
    const probed = new Set<string>();
    const timers = new Set<NodeJS.Timeout>();
    const releaseDay = (date: string) => {
      probed.add(date);
      for (const release of held[date]?.splice(0) ?? []) release();
    };
    t.after(() => {
      for (const date of Object.keys(held)) releaseDay(date);
      for (const timer of timers) clearTimeout(timer);
    });
    let active = 0;
    let swiped = false;
    const route = (date: string, n: number) => {
      const on = Date.parse(`${date}T20:00:00Z`) + n * 60000;
      const ended = `itinerary-${date}-${n}` !== swipe || swiped;
      return {
        itineraryId: `itinerary-${date}-${n}`,
        transporterId: `driver-${date}-${n}`,
        routeCode: `CX${n}`,
        companyId: 'provider-1',
        executionStatus: 'COMPLETE',
        stopProgress: { total: 1, completed: 1 },
        breaks: [
          {
            punchId: `punch-${n}`,
            breakId: `meal-${n}`,
            type: 'MEAL',
            state: ended ? 'OFF' : 'ON',
            timeStampOn: on,
            timeStampOff: ended ? on + 1800000 : null,
            sequenceNumber: 1,
          },
        ],
      };
    };
    const routes = (date: string) =>
      Array.from({ length: sizes[date]! + (date === busy && lists[date]! > 1 ? 1 : 0) }, (_, i) =>
        route(date, i + 1),
      );
    const { f, dsp, owner, collect } = await site(t, async (url) => {
      if (url.pathname.startsWith(itineraryApi)) {
        const id = url.searchParams.get('itineraryId')!;
        const date = id.slice('itinerary-'.length, 'itinerary-'.length + 10);
        reads[id] = (reads[id] || 0) + 1;
        if (id === swipe) swiped = true;
        peaks[date] = Math.max(peaks[date] ?? 0, ++active);
        // Hold the initial requests until the test observes the lane count. Bulk
        // completeness reads need no artificial delay after that overlap probe.
        if (!probed.has(date))
          await new Promise<void>((resolve) => {
            const release = () => {
              clearTimeout(timer);
              timers.delete(timer);
              resolve();
            };
            const timer = setTimeout(release, 15000);
            timers.add(timer);
            (held[date] ??= []).push(release);
          });
        active--;
        const c = routes(date).find((r) => r.itineraryId === id)!;
        const meal = c.breaks[0]!;
        return {
          json: {
            itineraryDetails: {
              ...c,
              localDate: date.split('-').map(Number),
              serviceAreaId: 'area-1',
              unknownStops: [],
              inactiveTasks: [],
              stops: [
                {
                  tasks: [
                    delivered(`before-${id}`, meal.timeStampOn - 60000),
                    delivered(`after-${id}`, meal.timeStampOn + 1860000),
                  ],
                },
              ],
            },
          },
        };
      }
      const date = url.searchParams.get('selectedDay')!;
      if (url.pathname.includes('/documentType/')) pages[date] = (pages[date] ?? 0) + 1;
      else lists[date] = (lists[date] ?? 0) + 1;
      return { html: executionPage(listProps(date, routes(date))) };
    });
    for (const [date, size] of Object.entries(sizes)) {
      const collecting = collect(date, `every-route-${date}`, 90000);
      void collecting.catch(() => {});
      try {
        await until(async () => (held[date]?.length ?? 0) >= Math.min(3, size), 30000);
        // A real core round trip while the provider reads are held also proves
        // that collection leaves the API responsive.
        assert.equal((await owner.get('/api/platform/health')).status, 200);
        assert.equal(peaks[date], Math.min(3, size));
      } finally {
        releaseDay(date);
      }
      const job = await collecting;
      assert.equal(job.status, 'succeeded', JSON.stringify(job));
      const count = size + (date === busy ? 1 : 0);
      const published = (await owner.get(`/api/dsp/cortex/meal-breaks?date=${date}`)).value;
      assert.equal(published[0].itineraryCount, count, `${date}: every route`);
      assert.equal(published[0].mealCount, count);
      assert.equal(published[0].verifiedGapPairs, count, `${date}: each meal keeps both gaps`);
      const read = Object.keys(reads).filter((id) => id.includes(date));
      assert.equal(read.length, count, `${date}: every route's itinerary was read`);
      assert.equal(peaks[date], Math.min(3, count), `${date}: as many at once as tabs, no more`);
      // The list's application moves to each route; only the other tabs load a route's
      // page, each for its first route.
      assert((pages[date] ?? 0) <= Math.min(3, count) - 1, JSON.stringify(pages));
      f.database(`dsps/${dsp.id}/data/cortex/cortex.sqlite`, (db) => {
        const rows = db
          .prepare(
            `SELECT i.transporter_id driver,r.started_at,r.ended_at,r.last_delivery_at,r.first_delivery_at
               FROM meal_itineraries i JOIN meal_records r USING(publication_id,itinerary_id)
              WHERE i.publication_id=? ORDER BY i.itinerary_id`,
          )
          .all(published[0].id) as any[];
        assert.deepEqual(
          rows.map((r) => r.driver).sort(),
          Array.from({ length: count }, (_, i) => `driver-${date}-${i + 1}`).sort(),
        );
        for (const r of rows) {
          const on = Date.parse(r.started_at);
          assert.equal(Date.parse(r.ended_at), on + 1800000, r.driver);
          assert.equal(Date.parse(r.last_delivery_at), on - 60000, r.driver);
          assert.equal(Date.parse(r.first_delivery_at), on + 1860000, r.driver);
        }
      });
    }
    assert.equal(reads[swipe], 2, 'The meal that ended while the day was read is read again');
    assert.equal(reads[`itinerary-${busy}-26`], 1, 'The rescue route is read once');
  },
);
test(
  "Cortex reads a route the application doesn't answer from its page",
  { skip: native, timeout: 180000 },
  async (t) => {
    // On the first day the route's first two itinerary requests fail, and the page it is
    // then read from first shows another route. On the second the application ignores
    // moves to the route.
    const failing = 'itinerary-2026-01-15-1';
    const still = 'itinerary-2026-01-16-1';
    const requests: Record<string, number> = {};
    const pages: Record<string, number> = {};
    // Each route's id names its day.
    const day = (id: string) => id.slice('itinerary-'.length, 'itinerary-'.length + 10);
    const route = (id: string) => {
      const on = Date.parse(`${day(id)}T20:00:00Z`);
      return {
        itineraryId: id,
        transporterId: id.replace('itinerary', 'driver'),
        routeCode: 'CX1',
        companyId: 'provider-1',
        executionStatus: 'COMPLETE',
        stopProgress: { total: 1, completed: 1 },
        breaks: [
          {
            punchId: 'punch-1',
            breakId: 'meal-1',
            type: 'MEAL',
            state: 'OFF',
            timeStampOn: on,
            timeStampOff: on + 1800000,
            sequenceNumber: 1,
          },
        ],
      };
    };
    const { f, dsp, collect, owner } = await site(t, async (url) => {
      if (url.pathname.startsWith(itineraryApi)) {
        const id = url.searchParams.get('itineraryId')!;
        requests[id] = (requests[id] ?? 0) + 1;
        if (id === failing && requests[id]! <= 2) return { status: 500 };
        const c = route(id);
        const on = c.breaks[0]!.timeStampOn;
        return {
          json: {
            itineraryDetails: {
              ...c,
              localDate: day(id).split('-').map(Number),
              serviceAreaId: 'area-1',
              unknownStops: [],
              inactiveTasks: [],
              stops: [
                {
                  tasks: [delivered('before', on - 60000), delivered('after', on + 1860000)],
                },
              ],
            },
          },
        };
      }
      const date = url.searchParams.get('selectedDay')!;
      const id = date === '2026-01-15' ? failing : still;
      const quirks: Quirks = date === '2026-01-15' ? {} : { still: [still] };
      if (url.pathname.includes('/documentType/')) {
        pages[id] = (pages[id] ?? 0) + 1;
        if (id === failing && pages[id] === 2) quirks.show = 'itinerary-2026-01-15-9';
      }
      return { html: executionPage(listProps(date, [route(id)]), quirks) };
    });
    for (const date of ['2026-01-15', '2026-01-16']) {
      const job = await collect(date, `unanswered-${date}`, 90000);
      assert.equal(job.status, 'succeeded', JSON.stringify(job));
      const published = (await owner.get(`/api/dsp/cortex/meal-breaks?date=${date}`)).value;
      assert.equal(published[0].verifiedGapPairs, 1, date);
      f.database(`dsps/${dsp.id}/data/cortex/cortex.sqlite`, (db) => {
        const record = db
          .prepare(
            'SELECT last_delivery_at,started_at,ended_at,first_delivery_at FROM meal_records WHERE publication_id=?',
          )
          .get(published[0].id);
        assert.deepEqual(
          { ...(record as object) },
          {
            last_delivery_at: `${date}T19:59:00.000Z`,
            started_at: `${date}T20:00:00.000Z`,
            ended_at: `${date}T20:30:00.000Z`,
            first_delivery_at: `${date}T20:31:00.000Z`,
          },
        );
      });
    }
    // Failed in the moved application and in its own loaded page, the route was read
    // from a page of its own, which was loaded again once it showed another route.
    assert.equal(requests[failing], 3);
    assert.equal(requests['itinerary-2026-01-15-9'], 1);
    assert.equal(pages[failing], 3);
    // Unanswered in the moved application, the route was read from its loaded page.
    assert.equal(requests[still], 1);
    assert.equal(pages[still], 1);
  },
);
