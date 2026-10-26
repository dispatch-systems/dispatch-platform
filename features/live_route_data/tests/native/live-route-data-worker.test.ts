import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, until } from '../../../../core/shell/tests/support/support.js';
import { routesSite } from '../../../../collectors/cortex/tests/support/routes-site.js';

const MINUTE = 60_000;
const EVERY = 5 * MINUTE;

// Live Route Data's reader as the server runs it, with a real browser, against a staged
// execution site. Its first read opens the DSP's browser, which reads the day natively and
// hands back the reader that read it: the next read uses that reader, with no browser and no
// page. Once that reader's key is refused and the page carries none, a read opens the browser
// again, which reads with the application's signatures, the site renewing the session's cookie
// meanwhile: the reader it hands back carries the renewed session, so the read after it reads
// natively again, with no browser. Once the DSP's credentials are replaced, that reader goes
// with the sign-in it came from: nothing is asked with its session again.
test(
  'Live Route Data reads natively with the reader its browser hands back, and the session the browser ends with',
  { skip: process.env.DISPATCH_TEST_NATIVE !== '1', timeout: 420000 },
  async (t) => {
    const day = '2026-09-25';
    // Where it's late morning, so the window keeps to one day: Etc/GMT-N is N hours ahead.
    const ahead = 11 - new Date().getUTCHours();
    const zone = `Etc/GMT${ahead > 0 ? '-' : '+'}${Math.abs(ahead)}`;
    const site = await routesSite(day, zone);
    t.after(site.close);
    site.keyed = true;
    const f = await fixture({
      env: {
        DISPATCH_FIXTURE_PROVIDER_URL: site.url,
        // Today is the site's day, wherever the clock is.
        DISPATCH_FIXTURE_TODAY: day,
        DISPATCH_BWRAP_EXECUTABLE:
          process.env.DISPATCH_BWRAP_EXECUTABLE ?? '/usr/local/libexec/dispatch-dev/bwrap',
      },
    });
    t.after(f.close);
    const owner = await f.client();
    const dsp = owner.session.dsps.find((d: any) => d.name === 'Northline Logistics');
    await owner.select(dsp.id);
    const profile = { name: 'Northline Logistics', abbreviation: 'NLL', stationCode: 'TST1' };
    const set = await owner.post('/api/dsp/profile', { ...profile, timezone: zone });
    assert.equal(set.status, 200, set.body);
    await owner.select(dsp.id);
    const saved = await owner.post('/api/dsp/connections/cortex', {
      username: 'fixture@example.test',
      password: 'fixture-password',
    });
    assert.equal(saved.value.status, 'ready', saved.body);
    // A snapshot collected first names the scope today's reads are in, and its job's end
    // closes the browser signing in opened.
    const queued = await owner.post('/api/dsp/routes/collect', {
      requestId: 'scope',
      date: day,
      mode: 'snapshot',
    });
    assert.equal(queued.status, 202, queued.body);
    await until(async () => {
      const jobs = await owner.read('/api/dsp/routes/jobs');
      const job = jobs.find((j: any) => j.id === queued.value[0].id);
      assert.notEqual(job.status, 'failed', JSON.stringify(job));
      return job.status === 'succeeded';
    }, 120000);

    // Each read's record: kept, its counts, and how it read.
    const reads = () =>
      f
        .database('data/preview/jobs.sqlite', (db) =>
          db
            .prepare('SELECT outcome,code,counts FROM live_reads WHERE dsp_id=? ORDER BY at')
            .all(dsp.id),
        )
        .map((read: any) => ({ ...read, counts: JSON.parse(read.counts ?? 'null') }));
    const read = async (n: number) => {
      await until(async () => reads().length >= n, 2 * MINUTE);
      const read = reads()[n - 1]!;
      assert.equal(read.outcome, 'kept', JSON.stringify(read));
      assert.equal(read.counts.listed, 2);
      return Object.keys(read.counts)
        .filter((name) => name === 'browser' || name.startsWith('routes_'))
        .sort();
    };
    // What the site was asked for from `since` on: each document, and who sent each data
    // request.
    const asked = (since: { documents: number; requests: number }) => ({
      documents: site.documents.slice(since.documents),
      by: [...new Set(site.requests.slice(since.requests).map(({ by }) => by))],
    });
    const now = () => ({ documents: site.documents.length, requests: site.requests.length });
    const local = (at: number) => new Date(at + ahead * 3_600_000).toISOString().slice(11, 16);
    // Starts the window an interval before the minute after the next quarter, its next slot.
    const section = 'live_route_data.collection';
    const readSoon = async () => {
      const slot = Math.ceil((Date.now() + 15_000) / MINUTE) * MINUTE;
      const settings = await owner.read('/api/dsp/settings');
      const { revision } = settings.sections.find((s: { id: string }) => s.id === section);
      const values = { start: local(slot - EVERY), stop: '23:59' };
      const moved = await owner.post(`/api/dsp/settings/${section}`, { values, revision });
      assert.equal(moved.status, 200, moved.body);
    };

    // Switched on inside its window, it reads at once: its browser reads natively.
    const switched = await owner.post(`/api/platform/dsps/${dsp.id}/features`, {
      feature: 'live_route_data',
      enabled: true,
    });
    assert.equal(switched.status, 200, switched.body);
    await owner.select(dsp.id);
    assert.deepEqual(await read(1), ['browser', 'routes_native']);
    // The reader that browser handed back reads the next with its key: no browser, no page.
    let since = now();
    await readSoon();
    assert.deepEqual(await read(2), ['routes_native']);
    assert.deepEqual(asked(since), { documents: [], by: ['native'] });

    // Its key refused and the page carrying none, the next read opens the browser, which reads
    // with the application's signatures, the site renewing the session as the list loads.
    site.keyed = false;
    site.newKey();
    site.renewing = true;
    await readSoon();
    assert.deepEqual(await read(3), ['browser', 'routes_native_no_key', 'routes_signed']);
    assert.equal(site.renewing, false);
    assert.ok(site.refused.length > 0);
    // The reader that browser handed back carries the renewed session: it asks the page for its
    // key and reads natively, with no browser.
    site.keyed = true;
    since = now();
    await readSoon();
    assert.deepEqual(await read(4), ['routes_native']);
    assert.deepEqual(asked(since), {
      documents: [{ path: '/operations/execution/itineraries', browser: false }],
      by: ['native'],
    });

    // Its credentials replaced, signing in again starts another session, and the reader the
    // browser handed back is never used again: nothing is asked with the session it carried.
    const old = site.sessions.at(-1)!.cookie;
    const replaced = await owner.post('/api/dsp/connections/cortex', {
      username: 'second@example.test',
      password: 'fixture-password',
    });
    assert.equal(replaced.value.status, 'ready', replaced.body);
    const asking = site.sessions.length;
    await readSoon();
    await until(async () => reads().length >= 5, 2 * MINUTE);
    assert.deepEqual(
      site.sessions.slice(asking).filter(({ cookie }) => cookie === old),
      [],
    );
    // The next read opens a browser of its own, or finds the member's still open from signing in.
    const fifth = reads()[4]!;
    assert.ok(
      fifth.outcome === 'kept' ? fifth.counts.browser === 1 : fifth.code === 'connection_busy',
      JSON.stringify(fifth),
    );
  },
);
