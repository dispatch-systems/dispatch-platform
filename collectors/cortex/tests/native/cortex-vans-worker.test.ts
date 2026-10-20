import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import type { AddressInfo } from 'node:net';
import { fixture, until } from '../../../../core/shell/tests/support/support.js';
import { fileContainsAny } from '../../../../core/shell/tests/support/files.js';

// What only the collected vans carry, so any copy of them outside their database shows.
const MARKERS = ['VanMarker 214', 'VANMARK01'];

// The real Rust driver against a staged Cortex: a vans job starts from the session the
// connection's sign-in kept, reads the company list once, in the session check, reads the
// fleet's two pages over HTTP with every status, and leaves nothing outside Vans' database.
test(
  'Cortex BrowserOS collects the fleet from the saved session in two pages, reading the companies once',
  { skip: process.env.DISPATCH_TEST_NATIVE !== '1', timeout: 300000 },
  async (t) => {
    const seen = { companies: 0, fleet: [] as URL[], signIns: 0 };
    const van = (vin: string, name: string, status: string) => ({
      vin,
      dspVehicleId: name,
      registrationNo: name === MARKERS[0] ? MARKERS[1] : 'FXT0100',
      status,
      serviceTier: 'ELECTRIC_RPV_MEDIUM',
      serviceStation: { stationCode: 'TST1' },
    });
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
      const json = (value: object) => {
        res.setHeader('content-type', 'application/json;charset=UTF-8');
        res.end(JSON.stringify(value));
      };
      if (url.pathname === '/dspconsolev2') {
        return authenticated ? html('<a href="/ap/signin">Sign out</a>') : redirect('/ap/signin');
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
        seen.signIns++;
        res.setHeader('set-cookie', 'authenticated=yes; Path=/; Max-Age=3600');
        return redirect('/dspconsolev2');
      }
      if (url.pathname === '/workforce/listCompanies') {
        seen.companies++;
        if (!authenticated) {
          res.writeHead(401);
          return res.end();
        }
        return json({ records: [{ companyId: 'company-1', shortCode: 'NLL' }] });
      }
      if (url.pathname === '/fleet-management/api/vehicles') {
        if (!authenticated) {
          res.writeHead(401);
          return res.end();
        }
        seen.fleet.push(url);
        return url.searchParams.get('continuationKey') === 'page-2'
          ? json({
              data: {
                vehicles: [van('3C6LRVDG0ME000301', 'Rental 301', 'INACTIVE')],
                continuationKey: null,
              },
            })
          : json({
              data: {
                vehicles: [
                  van('1FTBW3XM5LKA00101', 'CDV 101', 'ACTIVE'),
                  van('7FCTGAAA1NN000214', MARKERS[0], 'ACTIVE'),
                ],
                continuationKey: 'page-2',
              },
            });
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
    const before = { ...seen, fleet: seen.fleet.length };

    const queued = await owner.post('/api/dsp/vans/collect', { requestId: 'fleet' });
    assert.equal(queued.status, 202, queued.body);
    await until(async () => {
      const status = await owner.read('/api/dsp/vans/status');
      const current = status.jobs.find((job: any) => job.id === queued.value.id);
      assert.notEqual(current?.status, 'failed', JSON.stringify(current));
      return current?.status === 'succeeded';
    }, 120000);
    // No sign-in, and the session check's company list served the collection too.
    assert.equal(seen.signIns, before.signIns);
    assert.equal(seen.companies, before.companies + 1);
    const pages = seen.fleet.slice(before.fleet);
    assert.deepEqual(
      pages.map((url) => [
        url.searchParams.get('vehicleStatuses'),
        url.searchParams.get('continuationKey'),
      ]),
      [
        ['ACTIVE,MAINTENANCE,PENDING,INACTIVE', null],
        ['ACTIVE,MAINTENANCE,PENDING,INACTIVE', 'page-2'],
      ],
    );
    const database = path.join(f.root, `dsps/${dsp.id}/data/vans/vans.sqlite`);
    const stored = f.database(`dsps/${dsp.id}/data/vans/vans.sqlite`, (db) =>
      db
        .prepare('SELECT vin,name,status,listed FROM vans ORDER BY vin')
        .all()
        .map((r) => ({ ...r })),
    );
    assert.deepEqual(stored, [
      { vin: '1FTBW3XM5LKA00101', name: 'CDV 101', status: 'ACTIVE', listed: 1 },
      { vin: '3C6LRVDG0ME000301', name: 'Rental 301', status: 'INACTIVE', listed: 1 },
      { vin: '7FCTGAAA1NN000214', name: MARKERS[0], status: 'ACTIVE', listed: 1 },
    ]);

    // Everything collected lives in Vans' database alone: no browser run is left, and no
    // other file holds any of it, in any encoding a browser stores text.
    const runs = path.join(f.root, 'data/preview/browser-runs');
    await until(async () => !fs.existsSync(runs) || fs.readdirSync(runs).length === 0, 30000);
    const needles = MARKERS.flatMap((m) => [Buffer.from(m, 'utf8'), Buffer.from(m, 'utf16le')]);
    const holding: string[] = [];
    const walk = (dir: string) => {
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        const file = path.join(dir, entry.name);
        if (entry.isDirectory()) walk(file);
        else if (entry.isFile() && !file.startsWith(database) && fileContainsAny(file, needles))
          holding.push(path.relative(f.root, file));
      }
    };
    walk(f.root);
    assert.deepEqual(holding, []);
  },
);
