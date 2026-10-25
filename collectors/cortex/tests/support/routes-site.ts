import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import http from 'node:http';
import type { AddressInfo } from 'node:net';

// Cortex's execution site for one made-up day of two drivers' routes at station TST1, as a
// routes collection and a live reader meet it: signing in, discovery's pages, the list's
// application, whose store signs and sends each data request as Cortex's does, and its data
// API, which refuses a request without a valid signature for its address. The list page
// carries the key a reader signs with itself (`hmacSession`) once `keyed`.

/** Who sent a data request: the page, the driver with the page's signature, or a reader
 * signing it with the page's key. */
export type Sender = 'page' | 'driver' | 'native';

export interface RoutesSite {
  /** Where it answers, as `DISPATCH_FIXTURE_PROVIDER_URL` names it. */
  url: string;
  area: string;
  company: string;
  /** Each data request, and who sent it. */
  requests: { url: URL; by: Sender }[];
  /** Data requests without a valid signature for their address, which Cortex refuses. */
  unsigned: string[];
  /** Data requests signed with a key it handed out before its current one, refused too. */
  refused: string[];
  /** Documents loaded other than the list: Cortex's pages are one application that fetches
   * what its address shows, on load and whenever its history changes. */
  pages: string[];
  /** Every document asked for, by its path: by a browser, or with no browser, as a reader
   * asks for the list's page for its key. */
  documents: { path: string; browser: boolean }[];
  /** Whether the list page carries a key. */
  keyed: boolean;
  /** Whether an itinerary read with the page's key is one the collection refuses. */
  unreadable: boolean;
  /** The downloads with the application's signature that never come; the page's own
   * requests read them. */
  failing: (url: URL) => boolean;
  /** Whether the next list the page asks for ends the session it signed in with and hands it
   * another, as Cortex may renew a session's cookie. */
  renewing: boolean;
  /** Hands out a new key from now on: reads signed with one before it are refused. */
  newKey(): void;
  close(): Promise<void>;
}

/** The site for `day`, its station in `timezone`. */
export async function routesSite(day: string, timezone: string): Promise<RoutesSite> {
  const area = '7b0d842b-699a-41a1-8881-d6dd5f89939d';
  const company = 'company-1';
  const base = Date.parse(`${day}T15:00:00Z`);
  const hour = 3_600_000;
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
      localDate: day.split('-').map(Number),
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
        geocode: { latitude: 34.01, longitude: -118.0, scope: 0 },
      },
    ],
    transporters: [transporters[i]],
    companies: summaries.companies,
    itineraryPartners: null,
    routePauseStatus: null,
  });
  // The key the list page carries once `keyed`, as Cortex's pages hold it in their state, and
  // how many it has handed out.
  const native = { session: 'native-session-1', key: 'fixture-key-1', keys: 1 };
  // The session cookie a sign-in sets, and how many it has handed out.
  const session = { cookie: 'session-1', cookies: 1 };
  // Cortex's signature of one request with the page's key: its method, path, sorted query,
  // session and moment.
  const signature = (url: URL, signedAs: string, stamp: string) => {
    const query = new URLSearchParams(
      [...url.searchParams].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
    );
    return createHmac('sha256', native.key)
      .update(
        [
          'GET',
          url.pathname,
          query.toString(),
          `x-cortex-session:${signedAs}`,
          `x-cortex-timestamp:${stamp}`,
          '',
          'x-cortex-session;x-cortex-timestamp',
        ].join('\n'),
      )
      .digest('base64');
  };
  // The application's own signature of one request: its address, the session and the
  // moment, which only the application can make and the server checks. A header of the
  // page's own tells the page's requests from the driver's, which send only the signature.
  const canonical = (url: URL, signedAs: string, stamp: string) =>
    [
      'GET',
      url.pathname,
      [...url.searchParams]
        .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
        .map(([k, v]) => `${k}=${encodeURIComponent(v)}`)
        .join('&'),
      `x-cortex-session:${signedAs}`,
      `x-cortex-timestamp:${stamp}`,
    ].join('\n');
  // The pages: React props for discovery and the meal script, the application's store,
  // whose API action signs and sends each request as Cortex's does, and a router that
  // asks it for what the address shows.
  const props = (extra: string) =>
    `<script>
      const root = document.querySelector('#root');
      const canonical = ${canonical.toString()};
      const request = (address) => new Promise((resolve, reject) => {
        const url = new URL(address, location.origin);
        const stamp = String(Date.now());
        const xhr = new XMLHttpRequest();
        xhr.open('GET', url.pathname + url.search);
        xhr.setRequestHeader('X-Cortex-Hmac-Signature', btoa(canonical(url, 'fixture-session', stamp)));
        xhr.setRequestHeader('X-Cortex-Timestamp', stamp);
        xhr.setRequestHeader('X-Cortex-Session', 'fixture-session');
        xhr.setRequestHeader('user-ref', 'cortex-webapp-user');
        xhr.setRequestHeader('X-Fixture-Page', 'yes');
        xhr.onload = () => (xhr.status === 200 ? resolve(xhr.responseText) : reject(new Error(String(xhr.status))));
        xhr.onerror = () => reject(new Error('failed'));
        xhr.send();
      });
      const store = { getState: () => ({}), subscribe: () => () => {}, dispatch: (action) => {
        const call = action['Call API'];
        if (!call) return action;
        return request(call.endpoint + '?' + new URLSearchParams({ ...call.params, historicalDay: 'true' }));
      } };
      root.__reactFiber$fixture = { memoizedProps: { selectedStation: { serviceAreaID: '${area}', defaultStationCode: 'TST1', timeZone: '${timezone}' },
        providerFilterOptions: [{ value: 'ALL_DRIVERS', label: 'All drivers' }, { value: '${company}', label: 'Northline Logistics' }],
        providerFilterValue: new URL(location.href).searchParams.get('provider'), serviceAreaId: '${area}', selectedDay: '${day}',
        isLoadingSummaries: false, allItinerarySummaries: [], transporterSummary: {} }, return: { memoizedProps: { store }, return: null } };
      ${extra}
    </script>`;
  const router = `
    const api = (endpoint, params) => store.dispatch({ 'Call API': { endpoint: '/operations/execution/api/' + endpoint, method: 'get', params } }).catch(() => {});
    const show = () => {
      const path = location.pathname;
      if (path.startsWith('/operations/execution/itineraries/')) {
        const id = decodeURIComponent(path.split('/')[4]);
        api('itineraries/' + encodeURIComponent(id), { documentType: 'Itinerary', itineraryId: id, serviceAreaId: '${area}' });
      } else if (path === '/operations/execution/dv/routes') {
        api('route-summaries', { localDate: '${day}', serviceAreaId: '${area}', statsFromSummaries: 'true' });
      } else {
        api('summaries', { localDate: '${day}', serviceAreaId: '${area}' });
      }
    };
    addEventListener('popstate', show);
    show();`;
  const site: Omit<RoutesSite, 'url' | 'close'> = {
    area,
    company,
    requests: [],
    unsigned: [],
    refused: [],
    pages: [],
    documents: [],
    keyed: false,
    unreadable: false,
    failing: () => false,
    renewing: false,
    newKey: () => {
      native.keys += 1;
      native.session = `native-session-${native.keys}`;
      native.key = `fixture-key-${native.keys}`;
    },
  };
  const server = http.createServer(async (req, res) => {
    const url = new URL(req.url!, 'http://fixture.test');
    const cookies = (req.headers.cookie ?? '').split(/;\s*/);
    const authenticated = cookies.includes(`authenticated=${session.cookie}`);
    const html = (body: string) => {
      res.setHeader('content-type', 'text/html');
      const state = site.keyed
        ? `<script type="a-state">${JSON.stringify({ hmacSession: { sessionKey: native.key, sessionId: native.session } })}</script>`
        : '';
      res.end(
        `<html><head><title>DSP Console</title></head><body><div id="root">${body}</div>${state}</body></html>`,
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
    if (!url.pathname.startsWith('/operations/execution/api/'))
      site.documents.push({
        path: url.pathname,
        browser: req.headers['sec-fetch-mode'] !== undefined,
      });
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
      res.setHeader('set-cookie', `authenticated=${session.cookie}; Path=/; Max-Age=3600`);
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
      const signedAs = String(req.headers['x-cortex-session'] ?? '');
      const stamp = String(req.headers['x-cortex-timestamp'] ?? '');
      const signed = String(req.headers['x-cortex-hmac-signature'] ?? '');
      const by: Sender = signedAs.startsWith('native-session')
        ? 'native'
        : req.headers['x-fixture-page'] === 'yes'
          ? 'page'
          : 'driver';
      site.requests.push({ url, by });
      if (by === 'native' && signedAs !== native.session) {
        site.refused.push(url.pathname);
        res.writeHead(403);
        return res.end();
      }
      const valid =
        by === 'native'
          ? signed === signature(url, signedAs, stamp)
          : Buffer.from(signed, 'base64').toString() === canonical(url, signedAs, stamp);
      if (!signedAs || !stamp || !valid) {
        site.unsigned.push(url.pathname);
        res.writeHead(400);
        return res.end();
      }
      if (by === 'driver' && site.failing(url)) {
        res.writeHead(503);
        return res.end();
      }
      if (url.pathname === '/operations/execution/api/summaries') {
        if (by === 'page' && site.renewing) {
          site.renewing = false;
          session.cookies += 1;
          session.cookie = `session-${session.cookies}`;
          res.setHeader('set-cookie', `authenticated=${session.cookie}; Path=/; Max-Age=3600`);
        }
        return json(summaries);
      }
      if (url.pathname === '/operations/execution/api/route-summaries') return json(routeSummaries);
      if (url.pathname.startsWith('/operations/execution/api/itineraries/')) {
        const id = url.searchParams.get('itineraryId');
        const index = drivers.findIndex((d) => d.itinerary === id);
        if (index < 0) {
          res.writeHead(404);
          return res.end();
        }
        const read = detail(drivers[index]!, index);
        if (by === 'native' && site.unreadable)
          return json({ ...read, itineraryDetails: { ...read.itineraryDetails, stops: 'none' } });
        return json(read);
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
      return html(props(router));
    }
    if (
      url.pathname.startsWith('/operations/execution/itineraries/') ||
      url.pathname === '/operations/execution/dv/routes'
    ) {
      site.pages.push(url.pathname);
      return html(props(router));
    }
    res.writeHead(404);
    res.end();
  });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  return Object.assign(site, {
    url: `http://fixture.dispatch.invalid:${(server.address() as AddressInfo).port}`,
    close: async () => {
      server.closeAllConnections();
      await new Promise<void>((resolve) => server.close(() => resolve()));
    },
  });
}
