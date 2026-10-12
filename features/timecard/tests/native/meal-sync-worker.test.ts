// Timecard's meal sync queues Paycom and Cortex together; both publish through BrowserOS.
import test from 'node:test';
import assert from 'node:assert/strict';
import { until } from '../../../../core/shell/tests/support/support.js';
import {
  paycomFixture,
  credentials,
} from '../../../../collectors/paycom/tests/support/browseros-paycom-fixture.js';
import {
  executionPage,
  itineraryApi,
  listResponse,
  summariesApi,
} from '../../../../collectors/cortex/tests/support/cortex-execution.js';

const native = process.env.DISPATCH_TEST_NATIVE !== '1';
test(
  'Sync Now discovers both sources and a bundled Cortex job collects every snapshotted station',
  { skip: native, timeout: 180000 },
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
              serviceAreaId: url.searchParams.get('serviceAreaId'),
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
      if (url.pathname === summariesApi) {
        res.setHeader('Content-Type', 'application/json');
        res.end(JSON.stringify(listResponse([candidate], { 'driver-1': 'Fixture Driver' })));
        return true;
      }
      if (!url.pathname.startsWith('/operations/execution/itineraries')) return false;
      paths.push(url.pathname + url.search);
      if (!url.searchParams.get('serviceAreaId')) {
        res.writeHead(302, { Location: '/operations/execution/' });
        res.end();
        return true;
      }
      res.setHeader('Content-Type', 'text/html');
      const area = url.searchParams.get('serviceAreaId')!;
      res.end(
        executionPage(
          {},
          {
            serviceAreaId: area,
            selectedStation: {
              serviceAreaID: area,
              defaultStationCode: area === 'area-2' ? 'ABC1' : 'TST1',
              timeZone: 'US/Pacific',
            },
            providerFilterOptions: [
              { value: 'ALL_DRIVERS', label: 'All Drivers' },
              { value: 'ALL_DSPS' },
              { value: 'provider-1', label: 'NLL' },
              { value: 'other-provider', label: 'Other' },
            ],
          },
        ),
      );
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
          abbreviation: 'NLL',
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
    // Exercise the persisted day-bundle format through the ordinary native scheduler,
    // driver, live writer and keeper. The API's period fan-out is covered at its own layer.
    const source = queued.value.jobs.find((job: any) => job.kind === 'cortex.meal_breaks.collect');
    const bundleId = 'job_cortex_scope_bundle';
    f.database('data/preview/jobs.sqlite', (db) => {
      db.exec('PRAGMA busy_timeout=5000');
      const inserted = db
        .prepare(
          `INSERT INTO jobs
        (id,dsp_id,environment,kind,status,available_at,created_at,release,actor_id,connection_revision,idempotency_key,request)
        SELECT ?,dsp_id,environment,kind,'queued',?,?,release,actor_id,connection_revision,?,?
        FROM jobs WHERE id=?`,
        )
        .run(
          bundleId,
          Date.now(),
          new Date().toISOString(),
          bundleId,
          JSON.stringify({
            date,
            scopeRequests: [
              {
                date,
                station: 'TST1',
                serviceAreaId: 'area-1',
                provider: 'provider-1',
                timezone: 'America/Los_Angeles',
              },
              {
                date,
                station: 'ABC1',
                serviceAreaId: 'area-2',
                provider: 'provider-1',
                timezone: 'America/Los_Angeles',
              },
            ],
          }),
          source.id,
        );
      assert.equal(Number(inserted.changes), 1);
    });
    let bundled: any;
    await until(async () => {
      bundled = (await owner.read('/api/dsp/jobs')).find((job: any) => job.id === bundleId);
      assert.notEqual(bundled?.status, 'failed', JSON.stringify(bundled));
      return bundled?.status === 'succeeded';
    }, 60000);
    const both = (await owner.get(`/api/dsp/cortex/meal-breaks?date=${date}`)).value;
    assert.equal(both.length, 2);
    assert.deepEqual(
      both.map((publication: any) => publication.station),
      ['ABC1', 'TST1'],
    );
    assert(
      both.every(
        (publication: any) => publication.jobId === bundleId && publication.mealCount === 1,
      ),
    );
    assert.equal(bundled.metrics.at(-1).itineraries, 2);
    assert.equal(bundled.metrics.at(-1).meals, 2);
  },
);
