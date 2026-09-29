import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import type { AddressInfo } from 'node:net';
import { fixture, until } from '../support/support.js';

// What only the collected rows carry, so any copy of them outside the databases shows.
const MARKERS = ['TBA-SCORECARD-MARKER-1', 'Scorecard Marker Driver', 'QUALITY-MARKER-NOTE'];
// The Saturday that ends each week, as daily datasets name the week they ask for.
const SATURDAYS: Record<string, string> = {
  '2026-08-29': '2026-W35',
  '2026-09-05': '2026-W36',
  '2026-09-12': '2026-W37',
  '2026-09-19': '2026-W38',
};

// The real Rust driver against a staged Cortex. The first job finds the API through the
// overview page's own first data request; the next ones read at the address the first
// stored, without loading the overview; when the API moves, a job finds it again; a
// dataset the API refuses fails the attempt for a retry; and nothing collected is left
// outside the databases.
test(
  'Cortex BrowserOS collects scorecard weeks at the stored API address, finds it again when it moves, and leaves nothing outside the databases',
  { skip: process.env.DISPATCH_TEST_NATIVE !== '1', timeout: 300000 },
  async (t) => {
    let version = 'v1';
    const requests: URL[] = [];
    const seen = { overview: 0 };
    // Weeks 38 and 36 are posted, 37 is not, and 35's returns are refused.
    const rows = (dataSetId: string, week: string): object[] => {
      if (week === '2026-W37') return [];
      switch (dataSetId) {
        case 'dsp_station_weekly_quality':
          return [
            {
              dsp_code: 'NLOG',
              station_code: 'TST1',
              data_date: week,
              dsp_final_score: 91,
              note: MARKERS[2],
            },
          ];
        case 'da_dsp_weekly_rts_deep_dive':
          return week === '2026-W38'
            ? [
                {
                  transporter_id: 'driver-1',
                  tracking_id: MARKERS[0],
                  data_date: week,
                  impacting_dcr: 'Y',
                  rts_reason_code: 'BUSINESS CLOSED',
                },
                {
                  transporter_id: 'driver-2',
                  tracking_id: 'TBA2',
                  data_date: week,
                  impacting_dcr: 'N',
                  rts_reason_code: 'CUSTOMER UNAVAILABLE',
                },
              ]
            : [];
        case 'da_dsp_station_weekly_performance':
          return [
            {
              transporter_id: 'driver-1',
              da_name: MARKERS[1],
              data_date: week,
              da_overall_score: week === '2026-W38' ? 95 : 90,
            },
          ];
        case 'da_dsp_daily_psb_stop':
          return [];
        default:
          return [{ dsp_code: 'NLOG', data_date: week }];
      }
    };
    const server = http.createServer(async (req, res) => {
      const url = new URL(req.url!, 'http://fixture.test');
      const authenticated = req.headers.cookie?.includes('authenticated=yes');
      const html = (body: string) => {
        res.setHeader('content-type', 'text/html');
        res.end(`<html><head><title>DSP Console</title></head><body>${body}</body></html>`);
      };
      const redirect = (path: string) => {
        res.writeHead(302, { location: path });
        res.end();
      };
      if (url.pathname === '/dspconsolev2') {
        if (authenticated)
          return html(
            '<nav><a href="/scheduling/calendar-view/week">Weekly schedule</a></nav><a href="/ap/signin">Sign out</a>',
          );
        return redirect('/ap/signin');
      }
      let body = '';
      for await (const chunk of req) body += chunk;
      const fields = new URLSearchParams(body);
      if (url.pathname === '/ap/signin')
        return html(
          '<form action="/ap/password" method="post"><input id="ap_email" name="email"><button type="submit" id="continue">Continue</button></form>',
        );
      if (url.pathname === '/ap/password')
        return html(
          '<form action="/ap/submit" method="post"><input id="ap_password" type="password" name="password"><button type="submit" id="signInSubmit">Sign in</button></form>',
        );
      if (url.pathname === '/ap/submit') {
        assert.equal(fields.get('password'), 'fixture-password');
        res.setHeader('set-cookie', 'authenticated=yes; Path=/; Max-Age=3600');
        return redirect('/dspconsolev2');
      }
      if (url.pathname === '/performance') {
        if (!authenticated) return redirect('/ap/signin');
        // Like Cortex, the overview settles on a station and company of its own
        // unless one is asked for; here it settles on the wrong station first.
        const station = url.searchParams.get('station');
        if (!station || !url.searchParams.get('companyId')) {
          const chosen = station ?? 'XYZ1';
          return redirect(
            `/performance?pageId=dsp_dashboard_overview&station=${chosen}&companyId=company-1&tabId=overview-dsp-weekly-tab&timeFrame=Weekly&to=2026-W38`,
          );
        }
        seen.overview++;
        return html(
          `<main>Overview</main><script>for(let i=0;i<12;i++) fetch('/performance/api/${version}/getData?dataSetId=dsp_station_weekly_quality&dsp=NLOG&from=2026-W38&station=${station}&timeFrame=Weekly&to=2026-W38',{credentials:'include'});</script>`,
        );
      }
      const api = url.pathname.match(/^\/performance\/api\/([^/]+)\/getData$/);
      if (api) {
        if (!authenticated) {
          res.writeHead(401);
          return res.end();
        }
        // An address Cortex no longer serves answers as an unknown page does.
        if (api[1] !== version) {
          res.writeHead(404, { 'content-type': 'text/html' });
          return res.end('<html>Not found</html>');
        }
        requests.push(url);
        const dataSetId = url.searchParams.get('dataSetId')!;
        const to = url.searchParams.get('to')!;
        const week = url.searchParams.get('timeFrame') === 'Weekly' ? to : (SATURDAYS[to] ?? '');
        // Refused in a way no page can answer either: the attempt fails for a retry.
        if (week === '2026-W35' && dataSetId === 'da_dsp_weekly_rts_deep_dive') {
          res.writeHead(404);
          return res.end();
        }
        res.setHeader('content-type', 'application/json;charset=UTF-8');
        return res.end(
          JSON.stringify({
            tableData: {
              [dataSetId]: {
                rows: rows(dataSetId, week).map((r) => JSON.stringify(r)),
              },
            },
          }),
        );
      }
      res.writeHead(404);
      res.end();
    });
    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
    t.after(async () => {
      server.closeAllConnections();
      await new Promise<void>((resolve) => server.close(() => resolve()));
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
    const saved = await owner.post('/api/dsp/connections/cortex', {
      username: 'fixture@example.test',
      password: 'fixture-password',
    });
    assert.equal(saved.value.status, 'ready', saved.body);
    const job = async (id: string) =>
      (await owner.get('/api/dsp/jobs')).value.find((j: any) => j.id === id);
    const collect = async (requestId: string, week: string) => {
      const queued = await owner.post('/api/dsp/scorecard/collect', { requestId, week });
      assert.equal(queued.status, 202, queued.body);
      await until(async () => {
        const current = await job(queued.value.id);
        assert.notEqual(current.status, 'failed', JSON.stringify(current));
        return current.status === 'succeeded';
      }, 120000);
      return queued.value.id;
    };
    const stored = () =>
      f.database(`dsps/${dsp.id}/data/scorecard/scorecard.sqlite`, (db) => ({
        publications: db
          .prepare(
            'SELECT job_id,week,company_id,active FROM scorecard_publications ORDER BY rowid',
          )
          .all()
          .map((r) => ({ ...r })),
        urls: db
          .prepare(
            "SELECT p.week,s.url FROM scorecard_sources s JOIN scorecard_publications p ON p.id=s.publication_id WHERE s.dataset='dsp_station_weekly_quality' ORDER BY p.rowid",
          )
          .all()
          .map((r) => ({ ...r }) as { week: string; url: string }),
        returns: db
          .prepare(
            "SELECT tracking_id,impact,json_extract(row,'$.rts_reason_code') reason FROM returns_to_station ORDER BY rowid",
          )
          .all()
          .map((r) => ({ ...r })),
      }));

    // The first job has no stored address: the overview page names the API.
    const first = await collect('week-38', '2026-W38');
    const overviews = seen.overview;
    assert.ok(overviews > 0);
    const weeks = (await owner.get('/api/dsp/scorecard/weeks')).value;
    const posted = weeks.weeks.find((w: any) => w.week === '2026-W38');
    assert.equal(posted.posted, true);
    assert.equal(posted.publication.dspCode, 'NLOG');
    assert.equal(posted.publication.rowCount, 14);
    const read = new Map(requests.map((u) => [u.searchParams.get('dataSetId')!, u]));
    assert.equal(read.size, 14, [...read.keys()].join(','));
    for (const u of read.values()) {
      assert.equal(u.searchParams.get('dsp'), 'NLOG');
      assert.equal(u.searchParams.get('station'), 'TST1');
    }
    assert.equal(
      read.get('da_dsp_station_weekly_performance')!.searchParams.get('program'),
      'AMZL',
    );
    assert.equal(read.get('da_dsp_daily_psb_stop')!.searchParams.get('from'), '2026-09-13');
    assert.equal(read.get('da_dsp_daily_psb_stop')!.searchParams.get('to'), '2026-09-19');
    assert.deepEqual(stored().publications, [
      { job_id: first, week: '2026-W38', company_id: 'company-1', active: 1 },
    ]);
    assert.deepEqual(stored().returns, [
      { tracking_id: MARKERS[0], impact: 1, reason: 'BUSINESS CLOSED' },
      { tracking_id: 'TBA2', impact: 0, reason: 'CUSTOMER UNAVAILABLE' },
    ]);

    // The next jobs read at the stored address without loading the overview: a week not
    // posted yet is noted, a posted one published.
    requests.length = 0;
    await collect('week-37', '2026-W37');
    const posted36 = await collect('week-36', '2026-W36');
    assert.equal(seen.overview, overviews);
    assert.equal(requests.length, 14 * 2);
    assert.ok(requests.every((u) => u.pathname === '/performance/api/v1/getData'));
    const after = (await owner.get('/api/dsp/scorecard/weeks')).value.weeks;
    assert.equal(after.find((w: any) => w.week === '2026-W37').posted, false);
    const w36 = after.find((w: any) => w.week === '2026-W36');
    assert.equal(w36.posted, true);
    assert.equal(
      w36.publication.datasets.find((d: any) => d.table === 'driver_scorecards').rows,
      1,
    );
    assert.deepEqual(
      stored().publications.filter((p) => p.job_id === posted36),
      [{ job_id: posted36, week: '2026-W36', company_id: 'company-1', active: 1 }],
    );
    assert.equal((await job(posted36)).metrics.at(-1).rows, 12);

    // Cortex moves its API: nothing answers at the stored address, the page names the
    // new one, and the week is read there.
    version = 'v2';
    await collect('week-38-again', '2026-W38');
    assert.ok(seen.overview > overviews);
    assert.match(stored().urls.at(-1)!.url, /\/performance\/api\/v2\/getData\?/);

    // A dataset the API refuses has no other source: the attempt fails for a retry.
    const refused = await owner.post('/api/dsp/scorecard/collect', {
      requestId: 'week-35',
      week: '2026-W35',
    });
    await until(async () => {
      const current = await job(refused.value.id);
      return current.error === 'scorecard_api_unreadable';
    }, 120000);
    const retried = await job(refused.value.id);
    assert.equal(retried.status, 'queued');
    assert.equal((await owner.post(`/api/dsp/jobs/${refused.value.id}/cancel`, {})).status, 200);
    assert.equal(
      stored().publications.some((p) => p.week === '2026-W35'),
      false,
    );

    // Everything collected lives in the scorecard database alone: no browser run is
    // left, and no other file holds any of it, in any encoding a browser stores text.
    const runs = path.join(f.root, 'data/preview/browser-runs');
    await until(async () => !fs.existsSync(runs) || fs.readdirSync(runs).length === 0, 30000);
    const database = path.join(f.root, `dsps/${dsp.id}/data/scorecard/scorecard.sqlite`);
    const needles = MARKERS.flatMap((m) => [Buffer.from(m, 'utf8'), Buffer.from(m, 'utf16le')]);
    const holding: string[] = [];
    const walk = (dir: string) => {
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        const file = path.join(dir, entry.name);
        if (entry.isDirectory()) walk(file);
        else if (entry.isFile() && !file.startsWith(database)) {
          const bytes = fs.readFileSync(file);
          if (needles.some((n) => bytes.includes(n))) holding.push(path.relative(f.root, file));
        }
      }
    };
    walk(f.root);
    assert.deepEqual(holding, []);
  },
);
