import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, until } from '../../../../core/shell/tests/support/support.js';
import { routesSite } from '../support/routes-site.js';

// The real Rust driver against a staged execution site: discovery finds the service
// area and provider. Its list page carries no key to sign with at first, so the itinerary
// list is read as the page's own data response; the list's application then signs every
// itinerary and the newer routes page's list, which the driver downloads itself, and the
// itinerary and the routes page's list whose downloads keep failing are read by moving the
// application to them. Once the page carries a key, the day is read again with no page,
// every read signed by the driver, and read as before when what that read fails its
// checks. The day is published from each, and each job's metrics keep why it fell back.
test(
  'Cortex BrowserOS collects a day of routes signed natively, or as the application signs it, moving to what will not download',
  { skip: process.env.DISPATCH_TEST_NATIVE !== '1', timeout: 240000 },
  async (t) => {
    const day = '2026-09-25';
    const site = await routesSite(day, 'America/Los_Angeles');
    t.after(site.close);
    const { area, company, requests, unsigned, pages } = site;
    // The downloads with the application's signature that never come; the page's own
    // requests read them.
    site.failing = (url: URL) =>
      url.searchParams.get('itineraryId') === 'itinerary-2' ||
      url.pathname === '/operations/execution/api/route-summaries';
    const f = await fixture({
      env: {
        DISPATCH_FIXTURE_PROVIDER_URL: site.url,
        DISPATCH_BWRAP_EXECUTABLE:
          process.env.DISPATCH_BWRAP_EXECUTABLE ?? '/usr/local/libexec/dispatch-dev/bwrap',
      },
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
    const saved = await owner.post('/api/dsp/connections/cortex', {
      username: 'fixture@example.test',
      password: 'fixture-password',
    });
    assert.equal(saved.value.status, 'ready', saved.body);
    const collected = async (requestId: string) => {
      const queued = await owner.post('/api/dsp/routes/collect', { requestId, date: day });
      assert.equal(queued.status, 202, queued.body);
      let job: any;
      await until(async () => {
        job = (await owner.read('/api/dsp/routes/jobs')).find(
          (j: any) => j.id === queued.value[0].id,
        );
        assert.notEqual(job.status, 'failed', JSON.stringify(job));
        return job.status === 'succeeded';
      }, 120000);
      return job;
    };
    const job = await collected(day);
    // The page carried no key: the application's signatures read the day, and moves the rest.
    assert.equal(job.metrics.at(-1).method, 'routes_signed_moved');
    assert.equal(job.metrics.at(-1).fallback, 'routes_native_no_key');
    const days = (await owner.get('/api/dsp/routes/days')).value;
    assert.equal(days.days.length, 1);
    assert.equal(days.days[0].itineraryCount, 2);
    assert.equal(days.days[0].routeCount, 2);
    assert.equal(days.days[0].taskCount, 4);
    const view = (await owner.get(`/api/dsp/routes/days/${day}`)).value;
    assert.deepEqual(
      view.itineraries.map((i: any) => [i.driverName, i.delivered, i.pickedUp]),
      [
        ['Fixture Driver', 1, 1],
        ['Second Driver', 1, 1],
      ],
    );
    // Every data request was signed for its own address, for this day and area. Lists
    // were the pages' own as discovery and the list loaded them; the first itinerary was
    // downloaded once, with nothing of the page's but the signature; the second
    // itinerary's and the routes page list's downloads were each signed again once, then
    // the page itself read them.
    assert.deepEqual(unsigned, []);
    for (const { url } of requests) assert.equal(url.searchParams.get('serviceAreaId'), area);
    const sent = (path: string, id?: string) =>
      requests
        .filter(({ url }) => url.pathname.startsWith(path))
        .filter(({ url }) => !id || url.searchParams.get('itineraryId') === id)
        .map(({ by }) => by);
    assert.ok(sent('/operations/execution/api/summaries').length > 0);
    assert.ok(sent('/operations/execution/api/summaries').every((by) => by === 'page'));
    assert.deepEqual(sent('/operations/execution/api/route-summaries').slice(-3), [
      'driver',
      'driver',
      'page',
    ]);
    assert.deepEqual(sent('/operations/execution/api/itineraries/', 'itinerary-1'), ['driver']);
    assert.deepEqual(sent('/operations/execution/api/itineraries/', 'itinerary-2'), [
      'driver',
      'driver',
      'page',
    ]);
    // The list's application moved to the itinerary it read: no itinerary's page was
    // loaded, and the routes page was loaded once, where discovery landed.
    assert.deepEqual(
      pages.filter((p) => p.startsWith('/operations/execution/itineraries/')),
      [],
    );
    assert.ok(
      pages.filter((p) => p === '/operations/execution/dv/routes').length <= 1,
      JSON.stringify(pages),
    );
    const stored = f.database(`dsps/${dsp.id}/data/routedata/routedata.sqlite`, (db) => ({
      publication: db
        .prepare('SELECT job_id,provider,service_area_id,active FROM route_publications')
        .all()
        .map((r) => ({ ...r })),
      tasks: db
        .prepare(
          "SELECT tracking_id,task_state,transporter_id FROM tasks WHERE task_type='DROP_OFF' ORDER BY transporter_id",
        )
        .all()
        .map((r) => ({ ...r })),
    }));
    assert.deepEqual(stored.publication, [
      { job_id: job.id, provider: company, service_area_id: area, active: 1 },
    ]);
    assert.deepEqual(stored.tasks, [
      { tracking_id: 'TBA000000000001', task_state: 'DELIVERED', transporter_id: 'driver-1' },
      { tracking_id: 'TBA000000000002', task_state: 'DELIVERED', transporter_id: 'driver-2' },
    ]);

    // A day read with the page's key that fails the collection's checks is read as before:
    // here every itinerary downloads, and only the routes page's list is read by a move.
    site.keyed = true;
    site.unreadable = true;
    site.failing = (url) => url.pathname === '/operations/execution/api/route-summaries';
    const checked = requests.length;
    const refused = await collected('refused');
    assert.equal(refused.metrics.at(-1).method, 'routes_signed_moved');
    assert.equal(refused.metrics.at(-1).fallback, 'routes_native_invalid');
    assert.deepEqual([...new Set(requests.slice(checked).map(({ by }) => by))].sort(), [
      'driver',
      'native',
      'page',
    ]);

    // Once the list page carries a key, the day is read again with no page: each list and
    // itinerary asked for once, signed by the driver.
    site.unreadable = false;
    const asked = requests.length;
    const again = await collected('again');
    assert.equal(again.metrics.at(-1).method, 'routes_native');
    assert.equal(again.metrics.at(-1).fallback, undefined);
    assert.deepEqual(unsigned, []);
    assert.deepEqual(
      requests
        .slice(asked)
        .map(({ url, by }) => `${by} ${url.pathname}`)
        .sort(),
      [
        'native /operations/execution/api/itineraries/itinerary-1',
        'native /operations/execution/api/itineraries/itinerary-2',
        'native /operations/execution/api/route-summaries',
        'native /operations/execution/api/summaries',
      ],
    );
    const republished = f.database(`dsps/${dsp.id}/data/routedata/routedata.sqlite`, (db) =>
      db
        .prepare('SELECT job_id FROM route_publications WHERE active=1')
        .all()
        .map((r: any) => r.job_id),
    );
    assert.deepEqual(republished, [again.id]);
  },
);
