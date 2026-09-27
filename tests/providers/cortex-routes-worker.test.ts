import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import type { AddressInfo } from 'node:net';
import { fixture, until } from '../support/support.js';

// The real Rust driver against a staged execution site: discovery finds the service
// area and provider, the itinerary list, the newer routes page and each itinerary are
// read as the pages' own data responses, and the day is published from them.
test(
  "Cortex BrowserOS collects a day of routes from the pages' own data responses",
  { skip: process.env.DISPATCH_TEST_NATIVE !== '1', timeout: 240000 },
  async (t) => {
    const area = '7b0d842b-699a-41a1-8881-d6dd5f89939d';
    const company = 'company-1';
    const day = '2026-09-25';
    const base = Date.UTC(2026, 8, 25, 15);
    const hour = 3_600_000;
    const requests: URL[] = [];
    const drivers = [
      {
        id: 'driver-1',
        first: 'Fixture',
        last: 'Driver',
        route: 'CX101',
        itinerary: 'itinerary-1',
      },
      { id: 'driver-2', first: 'Second', last: 'Driver', route: 'CX102', itinerary: 'itinerary-2' },
    ];
    const transporters = drivers.map((d) => ({
      transporterId: d.id,
      firstName: d.first,
      lastName: d.last,
      initials: 'FD',
      workPhoneNumber: '15550000000',
      personType: ['DELIVERY_ASSOCIATE'],
      lastLocation: null,
    }));
    const summaries = {
      itinerarySummaries: drivers.map((d) => ({
        itineraryId: d.itinerary,
        transporterId: d.id,
        companyId: company,
        routeCode: d.route,
        executionStatus: 'COMPLETE',
        progressStatus: 'ON_TIME',
        plannedDepartureTime: base,
        itineraryStartTime: base + 600_000,
        sessionEndTime: base + 8 * hour,
        totalLocations: 2,
        completedLocations: 2,
        totalPackages: 2,
        packageSummary: { DELIVERED: 2, REMAINING: 0, UNDELIVERABLE: 0 },
        totalBreaksDurationSecs: 1800,
        stopProgress: { completed: 2, total: 2 },
        breaks: [],
        plannedBreaks: [],
      })),
      transporters,
      companies: [{ companyId: company, companyName: 'Northline Logistics', id: company }],
      transporterPackageSummaries: [],
      workHourAnomalies: [],
      backendTime: base,
    };
    const routeSummaries = {
      rmsRouteSummaries: drivers.map((d, i) => ({
        routeId: `${1000 + i}`,
        rmsRouteId: `rms-${i + 1}`,
        routeCode: d.route,
        serviceAreaId: area,
        companyId: company,
        routeStatus: 'COMPLETE',
        progressStatus: 'ON_TIME',
        plannedDepartureTime: base,
        routeCreationTime: base - 6 * hour,
        routeDuration: 28800,
        totalStops: 2,
        totalTasks: 2,
        unassignedStops: 0,
        unassignedPackages: 0,
        lateDeparting: false,
        preDispatch: false,
        sameDayRoute: false,
        transporters: [{ transporterId: d.id, itineraryId: d.itinerary }],
      })),
      transporters,
      companies: summaries.companies,
      transporterPackageSummaries: [],
      workHourAnomalies: [],
      backendTime: base,
    };
    const task = (
      index: number,
      kind: string,
      state: string,
      address: string,
      when: number,
      tracking: string,
    ) => ({
      taskId: `task-${index}`,
      transporterId: null,
      taskType: kind,
      taskState: state,
      taskStateContext: state === 'DELIVERED' ? 'DELIVERED_TO_DOORSTEP' : 'NONE',
      executionStatus: 'COMPLETE',
      addressId: address,
      windowStartTime: base,
      windowEndTime: base + 12 * hour,
      actualExecutionTime: when,
      executionGeocode: { latitude: 34.01, longitude: -118.0, scope: 0 },
      recentTaskEvents: [{ taskState: state, taskStateContext: 'NONE', executionTime: when }],
      length: '10.0',
      width: '5.0',
      height: '4.0',
      weight: '2.5',
      volumeUnit: 'IN',
      weightUnit: 'LB',
      boxType: 'SMALL_ENVELOPE',
      timeWindowed: false,
      highValueTaskFlag: false,
      customerReturn: false,
      addressTypeInfo: 'RESIDENTIAL',
      domainMap: { scannableId: tracking, orderId: `order-${index}`, SOURCE: 'AMAZON' },
    });
    const detail = (d: (typeof drivers)[number], i: number) => ({
      itineraryDetails: {
        itineraryId: d.itinerary,
        transporterId: d.id,
        transporterName: `${d.first} ${d.last}`,
        serviceAreaId: area,
        companyId: company,
        localDate: [2026, 9, 25],
        routeCode: d.route,
        executionStatus: 'COMPLETE',
        progressStatus: 'ON_TIME',
        itineraryStartTime: base + 600_000,
        plannedDepartureTime: base,
        sessionEndTime: base + 8 * hour,
        stopProgress: { completed: 2, total: 2 },
        transporterTimeAttributes: {
          actualDepartureTime: base + 900_000,
          plannedDepartureTime: base,
        },
        breaks: [],
        plannedBreaks: [],
        inactiveTasks: [],
        unknownStops: [],
        stops: [
          {
            stopId: `stop-${i}-1`,
            sequenceNumber: 1,
            addressId: 'station',
            itineraryStopType: 'PICK_UP',
            routeCode: d.route,
            plannedStartTime: base,
            plannedEndTime: base + 600_000,
            dispatchStopFlag: true,
            tasks: [
              task(10 * i + 1, 'PICK_UP', 'PICKED_UP', 'station', base, `TBA00000000000${i + 1}`),
            ],
          },
          {
            stopId: `stop-${i}-2`,
            sequenceNumber: 2,
            addressId: `address-${i + 1}`,
            itineraryStopType: 'DROP_OFF',
            routeCode: d.route,
            plannedStartTime: base + 2 * hour,
            plannedEndTime: base + 2 * hour + 600_000,
            highValueStopFlag: false,
            tasks: [
              task(
                10 * i + 2,
                'DROP_OFF',
                'DELIVERED',
                `address-${i + 1}`,
                base + 2 * hour,
                `TBA00000000000${i + 1}`,
              ),
            ],
          },
        ],
        weatherData: null,
      },
      addresses: [
        {
          addressId: `address-${i + 1}`,
          address1: `${i + 1}00 Example St`,
          address2: null,
          address3: null,
          city: 'Fixture',
          state: 'CA',
          postalCode: '90000',
          customerName: null,
          customerPhone: null,
          geocode: { latitude: 34.01, longitude: -118.0, scope: 0 },
        },
      ],
      transporters: [transporters[i]],
      companies: summaries.companies,
      itineraryPartners: null,
      routePauseStatus: null,
    });
    // The pages: React props for discovery and the meal script, and scripts that fetch
    // the data the way Cortex's pages do, so the driver can take the responses.
    const props = (extra: string) =>
      `<script>
        const root = document.querySelector('#root');
        root.__reactFiber$fixture = { memoizedProps: { selectedStation: { serviceAreaID: '${area}', defaultStationCode: 'TST1', timeZone: 'America/Los_Angeles' },
          providerFilterOptions: [{ value: 'ALL_DRIVERS', label: 'All drivers' }, { value: '${company}', label: 'Northline Logistics' }],
          providerFilterValue: new URL(location.href).searchParams.get('provider'), serviceAreaId: '${area}', selectedDay: '${day}',
          isLoadingSummaries: false, allItinerarySummaries: [], transporterSummary: {} }, return: null };
        ${extra}
      </script>`;
    const server = http.createServer(async (req, res) => {
      const url = new URL(req.url!, 'http://fixture.test');
      const authenticated = req.headers.cookie?.includes('authenticated=yes');
      const html = (body: string) => {
        res.setHeader('content-type', 'text/html');
        res.end(
          `<html><head><title>DSP Console</title></head><body><div id="root">${body}</div></body></html>`,
        );
      };
      const json = (value: unknown) => {
        res.setHeader('content-type', 'application/json;charset=UTF-8');
        res.end(JSON.stringify(value));
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
      if (!authenticated) {
        if (url.pathname.startsWith('/operations/execution/api/')) {
          res.writeHead(401);
          return res.end();
        }
        return redirect('/ap/signin');
      }
      if (url.pathname.startsWith('/operations/execution/api/')) {
        requests.push(url);
        if (url.pathname === '/operations/execution/api/summaries') return json(summaries);
        if (url.pathname === '/operations/execution/api/route-summaries')
          return json(routeSummaries);
        if (url.pathname.startsWith('/operations/execution/api/itineraries/')) {
          const id = url.searchParams.get('itineraryId');
          const index = drivers.findIndex((d) => d.itinerary === id);
          if (index < 0) {
            res.writeHead(404);
            return res.end();
          }
          return json(detail(drivers[index]!, index));
        }
        res.writeHead(404);
        return res.end();
      }
      if (
        url.pathname === '/operations/execution/itineraries' ||
        url.pathname === '/operations/execution'
      ) {
        // Without a service area this profile lands on the newer routes page with the
        // area it last showed, as Cortex does once that page has been opened.
        if (!url.searchParams.get('serviceAreaId'))
          return redirect(
            `/operations/execution/dv/routes?navMenuVariant=external&provider=ALL_DRIVERS&selectedDay=${day}&serviceAreaId=${area}`,
          );
        return html(
          props(
            `fetch('/operations/execution/api/summaries?historicalDay=true&localDate=${day}&serviceAreaId=${area}', { credentials: 'include' });`,
          ),
        );
      }
      if (url.pathname.startsWith('/operations/execution/itineraries/')) {
        const id = decodeURIComponent(url.pathname.split('/')[4]!);
        return html(
          props(
            `fetch('/operations/execution/api/itineraries/?documentType=Itinerary&historicalDay=true&itineraryId=${id}&serviceAreaId=${area}', { credentials: 'include' });`,
          ),
        );
      }
      if (url.pathname === '/operations/execution/dv/routes') {
        return html(
          props(
            `fetch('/operations/execution/api/route-summaries?historicalDay=true&localDate=${day}&serviceAreaId=${area}&statsFromSummaries=true', { credentials: 'include' });`,
          ),
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
    const queued = await owner.post('/api/dsp/routes/collect', { requestId: day, date: day });
    assert.equal(queued.status, 202, queued.body);
    const job = queued.value[0];
    await until(async () => {
      const current = (await owner.get('/api/dsp/jobs')).value.find((j: any) => j.id === job.id);
      assert.notEqual(current.status, 'failed', JSON.stringify(current));
      return current.status === 'succeeded';
    }, 120000);
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
    // Each data request was the page's own, for this day and area.
    const paths = requests.map((u) => u.pathname);
    assert.ok(paths.includes('/operations/execution/api/summaries'));
    assert.ok(paths.includes('/operations/execution/api/route-summaries'));
    assert.equal(
      paths.filter((p) => p.startsWith('/operations/execution/api/itineraries/')).length,
      2,
    );
    for (const u of requests) assert.equal(u.searchParams.get('serviceAreaId'), area);
    const stored = f.database(`dsps/${dsp.id}/data/itineraries/itineraries.sqlite`, (db) => ({
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
  },
);
