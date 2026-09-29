import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

// The scripts as the collection calls them: without the `;` that ends each file.
const source = (name: string) =>
  fs.readFileSync(`backend/src/browsers/cortex/${name}`, 'utf8').trim().replace(/;$/, '');
const script = source('meal.js');
const rules = source('meal_rules.js');
const hook = source('meal_hook.js');
const origin = 'https://logistics.amazon.com';
const scope = {
  date: '2026-09-15',
  station: 'TST1',
  serviceAreaId: 'area-1',
  provider: 'provider-1',
  timezone: 'America/Los_Angeles',
};
const start = Date.parse('2026-09-15T19:00:00Z');
const summary = (progress = 1) => ({
  itineraryId: 'itinerary-1',
  transporterId: 'driver-1',
  routeCode: 'CX1',
  companyId: 'provider-1',
  executionStatus: 'IN_PROGRESS',
  stopProgress: { total: 2, completed: progress },
  latestTaskExecutionTime: start + progress,
  lastStopExecutionTime: start + progress,
  breaks: [
    {
      punchId: 'punch-1',
      breakId: 'meal-1',
      type: 'MEAL',
      state: 'ON',
      timeStampOn: start,
      timeStampOff: null,
      sequenceNumber: 1,
    },
  ],
});
const page = (day = scope.date, progress = 1) => ({
  selectedDay: day,
  serviceAreaId: 'area-1',
  selectedStation: { serviceAreaID: 'area-1', defaultStationCode: 'TST1', timeZone: 'US/Pacific' },
  providerFilterValue: 'provider-1',
  providerFilterOptions: [{ value: 'provider-1' }],
  isLoadingSummaries: false,
  allItinerarySummaries: [summary(progress)],
  transporterSummary: { 'driver-1': { transporterName: 'Fixture Driver' } },
});
const href = (day = scope.date, detail = false) =>
  `${origin}/operations/execution/itineraries${detail ? '/itinerary-1/documentType/Itinerary' : ''}?provider=provider-1&selectedDay=${day}&serviceAreaId=area-1`;
function read(root: object, location: string, candidate?: object) {
  const result = vm.runInNewContext(`(${script})(input, ${rules})`, {
    URL,
    Intl,
    input: { origin, scope, kind: candidate ? 'detail' : 'list', candidate },
    location: { href: location },
    document: { querySelectorAll: () => [{ __reactFiber$fixture: { memoizedProps: root } }] },
  });
  return JSON.parse(JSON.stringify(result));
}
const detail = (day: number[], progress = 1) => ({
  ...page(scope.date, progress),
  isLoadingItineraryDetails: false,
  itineraryDetails: {
    ...summary(progress),
    localDate: day,
    serviceAreaId: 'area-1',
    stops: [],
    unknownStops: [],
    inactiveTasks: [],
  },
});

test('meal collection only reads the requested day', () => {
  const [candidate] = read(page(), href()).candidates;
  assert.equal(candidate.id, 'itinerary-1');
  // Neither a URL nor a page showing another day can supply the selected date.
  assert.deepEqual(read(page('2026-09-18'), href('2026-09-18')), {
    error: 'cortex_scope_mismatch',
    reason: 'url_scope',
  });
  assert.deepEqual(read(page('2026-09-18'), href()), {
    error: 'cortex_scope_mismatch',
    reason: 'page_scope',
  });
  assert.deepEqual(read(detail([2026, 9, 18]), href(scope.date, true), candidate), {
    error: 'cortex_scope_mismatch',
    reason: 'detail_date',
  });
  const itinerary = read(detail([2026, 9, 15]), href(scope.date, true), candidate).itinerary;
  assert.deepEqual(itinerary.meals, [
    { id: 'meal-1', start, end: null, lastDelivery: null, firstDelivery: null },
  ]);
});

test('delivery progress during a working day does not restart meal collection', () => {
  const [before] = read(page(scope.date, 1), href()).candidates;
  const [after] = read(page(scope.date, 2), href()).candidates;
  assert.equal(after.revision, before.revision);
  assert.equal(read(detail([2026, 9, 15], 2), href(scope.date, true), before).error, undefined);
  // A meal swipe is published evidence, so it still forces a fresh read.
  const ended = page(scope.date, 2);
  ended.allItinerarySummaries[0]!.breaks.push({
    ...ended.allItinerarySummaries[0]!.breaks[0]!,
    punchId: 'punch-2',
    state: 'OFF',
    timeStampOff: start + 1800000,
  } as never);
  const [swiped] = read(ended, href()).candidates;
  assert.notEqual(swiped.revision, before.revision);
  assert.deepEqual(
    read({ ...detail([2026, 9, 15], 2), ...ended }, href(scope.date, true), before),
    { error: 'cortex_source_changed', reason: 'route_changed' },
  );
});

// A finished route whose meal both deliveries bound, as the page renders it: each stop
// has the id the page gives it, which the itinerary response does not send.
const finished = () => {
  const props = page();
  const route = props.allItinerarySummaries[0]!;
  route.executionStatus = 'COMPLETE';
  route.breaks = [{ ...route.breaks[0]!, state: 'OFF', timeStampOff: start + 1800000 } as never];
  const task = (id: string, time: number) => ({
    taskId: id,
    taskType: 'DROP_OFF',
    taskState: 'DELIVERED',
    executionStatus: 'COMPLETE',
    actualExecutionTime: time / 1000,
    transporterId: null,
  });
  return {
    ...props,
    isLoadingItineraryDetails: false,
    itineraryDetails: {
      ...route,
      localDate: [2026, 9, 15],
      serviceAreaId: 'area-1',
      stopProgress: { total: 2, completed: 2 },
      stops: [
        { stopId: 'stop-1', tasks: [task('task-1', start - 60000)] },
        {
          stopId: 'stop-2',
          tasks: [task('task-1', start - 60000), task('task-2', start + 1860000)],
        },
      ],
      unknownStops: [],
      inactiveTasks: [task('removed', start - 7200000)],
    },
  };
};
// The itinerary response for a rendered page: its details, without the page's stop ids.
const response = (props: ReturnType<typeof finished>) =>
  JSON.stringify({
    itineraryDetails: {
      ...props.itineraryDetails,
      stops: props.itineraryDetails.stops.map(({ stopId: _, ...stop }) => stop),
    },
  });
const api = (id = 'itinerary-1') =>
  `/operations/execution/api/itineraries/?documentType=Itinerary&historicalDay=true&itineraryId=${id}&serviceAreaId=area-1`;
// A page with the hook installed, whose application requests with `XMLHttpRequest` or
// `fetch` and is answered from `answers`.
function hooked(answers: Record<string, { status: number; body: string }>) {
  const answer = (url: string) =>
    answers[new URL(url, origin).searchParams.get('itineraryId')!] ?? { status: 404, body: '' };
  class Request {
    readyState = 0;
    responseType = '';
    onreadystatechange: (() => void) | null = null;
    #url = '';
    #status = 0;
    #text = '';
    #listeners: (() => void)[] = [];
    get status() {
      return this.#status;
    }
    get responseText() {
      if (this.responseType !== '' && this.responseType !== 'text') throw new Error('state');
      return this.#text;
    }
    get response() {
      return this.responseType === 'json' ? JSON.parse(this.#text) : this.#text;
    }
    open(_method: string, url: string) {
      this.#url = url;
    }
    addEventListener(_type: string, listener: () => void) {
      this.#listeners.push(listener);
    }
    // Completes at once. The application's own handler runs before the hook's listener,
    // as one set before `open` does.
    send() {
      const { status, body } = answer(this.#url);
      this.readyState = 4;
      this.#status = status;
      this.#text = body;
      this.onreadystatechange?.();
      for (const listener of this.#listeners) listener();
    }
  }
  const context: any = {
    URL,
    Response,
    Request: globalThis.Request,
    XMLHttpRequest: Request,
    location: { origin, href: href(scope.date, true) },
    fetch: async (url: string) => {
      const { status, body } = answer(url);
      return new Response(body, { status });
    },
  };
  context.window = context;
  vm.runInNewContext(`(${hook})(${rules})`, context);
  const run = (code: string) => JSON.parse(JSON.stringify(vm.runInNewContext(code, context)));
  const candidate = read(page(), href()).candidates[0];
  return {
    context,
    expect: (route = candidate, at = origin) =>
      run(
        `window.__dispatchMeals.expect(${JSON.stringify({ candidate: route, scope, origin: at })})`,
      ),
    take: (id = 'itinerary-1') => run(`window.__dispatchMeals.take(${JSON.stringify(id)})`),
    // What the application sees of its request.
    request(id = 'itinerary-1', type = '') {
      const xhr = new context.XMLHttpRequest();
      let seen: unknown;
      xhr.responseType = type;
      xhr.onreadystatechange = () => {
        if (xhr.readyState === 4)
          seen = { status: xhr.status, body: type === 'json' ? xhr.response : xhr.responseText };
      };
      xhr.open('GET', api(id));
      xhr.send();
      return seen;
    },
  };
}
const without = (result: any) => {
  delete result.itinerary.observedAt;
  return result;
};

test("the hook reads an itinerary response with meal.js's rules and withholds it", () => {
  const props = finished();
  const [candidate] = read(props, href()).candidates;
  const rendered = without(read(props, href(scope.date, true), candidate));
  assert.deepEqual(rendered.itinerary.meals, [
    {
      id: 'meal-1',
      start,
      end: start + 1800000,
      lastDelivery: start - 60000,
      firstDelivery: start + 1860000,
    },
  ]);
  assert.equal(rendered.itinerary.deliveryCoverage, 'complete');
  for (const type of ['', 'json']) {
    const page = hooked({ 'itinerary-1': { status: 200, body: response(props) } });
    assert.equal(page.expect(candidate), true);
    // The application only ever sees a failed request, even when its handler runs first.
    assert.deepEqual(page.request('itinerary-1', type), {
      status: 0,
      body: type === 'json' ? null : '',
    });
    assert.deepEqual(without(page.take()), rendered, 'The same evidence as the rendered page');
    assert.equal(page.take(), null, 'Taken once');
  }
});

test('the hook keeps a response that arrives before its route is named, and only that', () => {
  const props = finished();
  const [candidate] = read(props, href()).candidates;
  const page = hooked({
    'itinerary-1': { status: 200, body: response(props) },
    'itinerary-2': { status: 200, body: response(props) },
  });
  // A freshly loaded page fetches its route before the collection names it.
  page.request('itinerary-2');
  page.request('itinerary-1');
  assert.equal(page.take(), null);
  page.expect(candidate);
  assert.equal(page.take().itinerary.id, 'itinerary-1');
  // Naming a route forgets every other response the page kept.
  page.expect({ ...candidate, id: 'itinerary-2' });
  assert.equal(page.take('itinerary-2'), null);
  // A response for a route other than the one named is not read for it.
  page.expect(candidate);
  page.request('itinerary-2');
  assert.equal(page.take(), null);
  assert.equal(page.take('itinerary-2'), null);
});

test('the hook leaves a failed request to the application and tells the collection', () => {
  const [candidate] = read(finished(), href()).candidates;
  const page = hooked({ 'itinerary-1': { status: 401, body: 'signed out' } });
  page.expect(candidate);
  assert.deepEqual(page.request(), { status: 401, body: 'signed out' });
  assert.deepEqual(page.take(), { unanswered: true });
});

test('the hook reads and withholds a fetched itinerary the same way', async () => {
  const props = finished();
  const [candidate] = read(props, href()).candidates;
  const page = hooked({
    'itinerary-1': { status: 200, body: response(props) },
    'itinerary-2': { status: 500, body: 'unavailable' },
  });
  page.expect(candidate);
  const fetched: Response = await page.context.fetch(api());
  assert.equal(fetched.status, 503);
  assert.equal(await fetched.text(), '');
  assert.equal(page.take().itinerary.deliveryCoverage, 'complete');
  const failed: Response = await page.context.fetch(api('itinerary-2'));
  assert.equal(failed.status, 500);
  assert.equal(await failed.text(), 'unavailable');
});

test('the hook only answers on Cortex, and reads a route that changed hands as changed', () => {
  const props = finished();
  const [candidate] = read(props, href()).candidates;
  const page = hooked({ 'itinerary-1': { status: 200, body: response(props) } });
  assert.equal(page.expect(candidate, 'https://www.amazon.com'), false, 'A sign-in page');
  // The list the collection read named another driver: the route moved after it was read.
  page.expect({ ...candidate, transporterId: 'driver-2' });
  page.request();
  assert.deepEqual(page.take(), { error: 'cortex_source_changed', reason: 'route_changed' });
  // The rendered page checks its own list first, so there another driver in its details
  // is a page yet to catch up.
  const stale = finished();
  stale.itineraryDetails.transporterId = 'driver-2';
  assert.deepEqual(read(stale, href(scope.date, true), candidate), {
    error: 'cortex_scope_mismatch',
    reason: 'detail_transporter',
  });
});

test('meal.js still needs the id the page gives each stop; the response has none', () => {
  const props = finished();
  const [candidate] = read(props, href()).candidates;
  const unnamed = finished();
  delete (unnamed.itineraryDetails.stops[0] as any).stopId;
  assert.deepEqual(read(unnamed, href(scope.date, true), candidate), {
    error: 'cortex_content_incomplete',
    reason: 'stops',
  });
  const page = hooked({ 'itinerary-1': { status: 200, body: response(props) } });
  page.expect(candidate);
  page.request();
  assert.equal(page.take().itinerary.deliveryCoverage, 'complete');
});
