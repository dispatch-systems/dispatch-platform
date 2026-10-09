import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

// The scripts as the collection calls them: without the `;` that ends each file.
const source = (name: string) =>
  fs.readFileSync(`collectors/cortex/scripts/${name}`, 'utf8').trim().replace(/;$/, '');
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
    {
      id: 'meal-1',
      start,
      end: null,
      lastDelivery: null,
      firstDelivery: null,
      lastDeliveryStop: null,
      firstDeliveryStop: null,
    },
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

test('a route whose meal punches the rules reject keeps its route, its meals unknown', () => {
  const on = summary().breaks[0]!;
  const off = { ...on, punchId: 'punch-2', state: 'OFF', timeStampOff: start + 1800000 };
  const cases: [string, object[]][] = [
    ['meal_break_id', [{ ...on, breakId: '' }]],
    ['meal_punch_id', [{ ...on, punchId: null }]],
    ['meal_start', [{ ...on, timeStampOn: 0 }]],
    ['meal_state', [{ ...on, state: 'PAUSED' }]],
    ['meal_off_without_end', [{ ...off, timeStampOff: null }]],
    ['meal_end_before_start', [{ ...off, timeStampOff: start - 1000 }]],
    ['meal_on_with_end', [{ ...on, timeStampOff: start + 1000 }]],
    ['meal_repeat_sequence', [on, { ...off, sequenceNumber: 2 }]],
    ['meal_repeat_conflict', [off, { ...off, timeStampOff: start + 900000 }]],
    ['meal_repeat_outside', [{ ...on, timeStampOn: start + 3600000 }, off]],
  ];
  for (const [reason, breaks] of cases) {
    const props = page();
    props.allItinerarySummaries[0]!.breaks = breaks as never;
    const [route] = read(props, href()).candidates;
    // Only this route goes without meals, named for the rule; the day is still read.
    assert.deepEqual([route.meals, route.unreadable], [[], reason], reason);
  }
});

test('a meal start pressed twice is one meal that began at the first press', () => {
  const on = summary().breaks[0]!;
  const again = (seconds: number) => ({
    ...on,
    punchId: 'punch-2',
    timeStampOn: start + seconds * 1000,
  });
  // While the meal runs, Cortex holds both starts open.
  for (const breaks of [
    [on, again(4)],
    [again(64), on],
  ]) {
    const props = page();
    props.allItinerarySummaries[0]!.breaks = breaks as never;
    const [route] = read(props, href()).candidates;
    assert.deepEqual(route.meals, [{ id: 'meal-1', start, end: null }]);
    assert.equal(route.unreadable, undefined);
  }
  // Once it ends, the first start is the completed meal and the second lies inside it.
  const props = page();
  props.allItinerarySummaries[0]!.breaks = [
    { ...on, state: 'OFF', timeStampOff: start + 1800000 },
    again(4),
  ] as never;
  assert.deepEqual(read(props, href()).candidates[0].meals, [
    { id: 'meal-1', start, end: start + 1800000 },
  ]);
  // Starts further apart are not one press: which began the meal is unknown.
  props.allItinerarySummaries[0]!.breaks = [on, again(301)] as never;
  assert.equal(read(props, href()).candidates[0].unreadable, 'meal_repeat_conflict');
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
const summaries = (day = scope.date) =>
  `/operations/execution/api/summaries?historicalDay=false&localDate=${day}&serviceAreaId=area-1`;
// The day's list as the summaries API sends it for `props`' routes.
const listed = (props: ReturnType<typeof page>) =>
  JSON.stringify({
    itinerarySummaries: props.allItinerarySummaries,
    transporters: [{ transporterId: 'driver-1', firstName: 'Fixture', lastName: 'Driver' }],
    companies: [{ companyId: 'provider-1' }],
  });
// A page with the hook installed, whose application requests with `XMLHttpRequest` or
// `fetch` and is answered from `answers`: by itinerary, or `list` for the day's list.
function hooked(answers: Record<string, { status: number; body: string }>) {
  const answer = (url: string) => {
    const address = new URL(url, origin);
    const key = address.pathname.endsWith('/summaries')
      ? 'list'
      : address.searchParams.get('itineraryId')!;
    return answers[key] ?? { status: 404, body: '' };
  };
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
    want: (route: object) =>
      run(`window.__dispatchMeals.want(${JSON.stringify({ candidate: route, scope, origin })})`),
    taken: () => run('window.__dispatchMeals.taken()'),
    list: () => run(`window.__dispatchMeals.list(${JSON.stringify({ scope, origin })})`),
    // What the application sees of its request: an itinerary's, or the address `id` names.
    request(id = 'itinerary-1', type = '') {
      const xhr = new context.XMLHttpRequest();
      let seen: unknown;
      xhr.responseType = type;
      xhr.onreadystatechange = () => {
        if (xhr.readyState === 4)
          seen = { status: xhr.status, body: type === 'json' ? xhr.response : xhr.responseText };
      };
      xhr.open('GET', id.startsWith('/') ? id : api(id));
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
      // Each delivery's stop by its place in the route, as Cortex's links name it. A
      // delivery repeated in an overlapping group stop keeps the first.
      lastDeliveryStop: 0,
      firstDeliveryStop: 1,
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
  page.expect({ ...candidate, id: 'itinerary-2' });
  const failed: Response = await page.context.fetch(api('itinerary-2'));
  assert.equal(failed.status, 500);
  assert.equal(await failed.text(), 'unavailable');
  assert.deepEqual(page.take('itinerary-2'), { unanswered: true });
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

test('a delivery the page cannot account for leaves both its time and its stop out', () => {
  const props = finished();
  const [candidate] = read(props, href()).candidates;
  // Conflicting copies of one task cannot bound a meal, so coverage is lost.
  props.itineraryDetails.stops[1]!.tasks[0]!.actualExecutionTime -= 1;
  const { itinerary } = read(props, href(scope.date, true), candidate);
  assert.equal(itinerary.deliveryCoverage, 'unavailable');
  assert.deepEqual(
    itinerary.meals.map((m: any) => [
      m.lastDelivery,
      m.firstDelivery,
      m.lastDeliveryStop,
      m.firstDeliveryStop,
    ]),
    [[null, null, null, null]],
  );
});

test("the hook reads the day's list from its response as meal.js reads it from the page", () => {
  const props = page();
  const app = hooked({ list: { status: 200, body: listed(props) } });
  assert.equal(app.list(), null, 'Nothing until the application has its list');
  // The application gets its list as sent.
  assert.deepEqual(app.request(summaries()), { status: 200, body: listed(props) });
  assert.deepEqual(app.list(), read(props, href()));
  // A list for another day or service area is not the scope's.
  app.request(summaries('2026-09-14'));
  assert.deepEqual(app.list(), { error: 'cortex_scope_mismatch', reason: 'list_scope' });
  // A route whose punches the rules reject is kept without meals, as meal.js keeps it.
  const rejected = page();
  rejected.allItinerarySummaries[0]!.breaks[0]!.timeStampOn = 0;
  const strict = hooked({ list: { status: 200, body: listed(rejected) } });
  strict.request(summaries());
  assert.deepEqual(strict.list(), read(rejected, href()));
  assert.equal(strict.list().candidates[0].unreadable, 'meal_start');
});

test("a route's response whose punches the rules reject reads as the route, its meals unknown", () => {
  const props = finished();
  const [candidate] = read(props, href()).candidates;
  props.itineraryDetails.breaks = [{ ...props.itineraryDetails.breaks[0]!, timeStampOn: 0 }];
  const unknown = {
    id: 'itinerary-1',
    transporterId: 'driver-1',
    driver: 'Fixture Driver',
    route: 'CX1',
    routeComplete: true,
    deliveryCoverage: 'unavailable',
    meals: [],
    unreadable: 'meal_start',
  };
  const page = hooked({ 'itinerary-1': { status: 200, body: response(props) } });
  page.expect(candidate);
  page.request();
  assert.deepEqual(without(page.take()).itinerary, unknown);
  // The rendered page reads it the same way.
  assert.deepEqual(without(read(props, href(scope.date, true), candidate)).itinerary, unknown);
});

test('the hook reads several routes it waits for at once, whatever order they arrive in', () => {
  const props = finished();
  const [first] = read(props, href()).candidates;
  // The same route under other ids, as other routes of the day.
  const route = (id: string) => {
    const other = finished();
    other.itineraryDetails.itineraryId = id;
    return { status: 200, body: response(other) };
  };
  const page = hooked({
    'itinerary-1': route('itinerary-1'),
    'itinerary-2': route('itinerary-2'),
    'itinerary-3': route('itinerary-3'),
  });
  const second = { ...first, id: 'itinerary-2' };
  const third = { ...first, id: 'itinerary-3' };
  // A freshly loaded page fetches its own route before the collection names it.
  page.request('itinerary-3');
  assert.equal(page.want(first), true);
  assert.equal(page.want(second), true);
  page.request('itinerary-2');
  page.request('itinerary-1');
  assert.equal(page.want(third), true, 'Read from the response that already came');
  const taken = page.taken();
  assert.deepEqual(Object.keys(taken).sort(), ['itinerary-1', 'itinerary-2', 'itinerary-3']);
  for (const [id, result] of Object.entries<any>(taken)) assert.equal(result.itinerary.id, id);
  assert.deepEqual(page.taken(), {}, 'Taken once');
});
