// Installed in the application's world before its scripts run. When the application
// fetches an itinerary the collection is waiting for, this reads that route's meal
// evidence from the response inside the page with meal.js's rules (`rules` is
// meal_rules.js), keeps only the result, and answers the application with a failed
// request, so the page never renders the itinerary. The collection may wait for several
// routes at once: the application fetches each route it is moved to, even while an earlier
// one is still loading. The day's list the application fetches is kept and let through,
// so the collection reads its routes without waiting for the page to settle. As with
// meal.js, no delivery history, package references, stop/task IDs, addresses, cookies or
// tokens leave the page: of the list, only what meal.js reads of it; of a route, only the
// four meal timestamps, their coverage and the places in the route of the bounding stops.
(rules) => {
  if (window.__dispatchMeals) return;
  const { fail, token, routeMeals, detail } = rules;
  const results = new Map();
  // The routes the collection is waiting for, by id.
  const waiting = new Map();
  // A response that arrived before its route was named, as on a freshly loaded page,
  // waits here until it is; it never leaves the page. Only the latest few are kept.
  const unclaimed = new Map();
  // The list response the application received, with the address it asked.
  let list = null;
  const read = (status, text, input) => {
    // The application answers a failed request itself; the collection loads the page.
    if (status !== 200) return { unanswered: true };
    try {
      return detail(JSON.parse(text).itineraryDetails, input.candidate, input.scope, false);
    } catch (error) {
      const allowed = ['cortex_content_incomplete', 'cortex_invalid_meal_evidence'];
      return allowed.includes(error.message)
        ? fail(error.message, error.reason ? `response_${error.reason}` : 'evidence')
        : fail('cortex_content_incomplete', 'script_error');
    }
  };
  // The list's routes for `scope`, as meal.js reads them from the page's props.
  const candidates = (scope) => {
    const asked = new URL(list.address, location.href).searchParams;
    if (asked.get('localDate') !== scope.date || asked.get('serviceAreaId') !== scope.serviceAreaId)
      return fail('cortex_scope_mismatch', 'list_scope');
    if (scope.provider === 'ALL_DSPS') return fail('invalid_cortex_scope', 'provider');
    try {
      const body = JSON.parse(list.text);
      const all = body.itinerarySummaries;
      if (!Array.isArray(all) || !Array.isArray(body.transporters))
        return fail('cortex_content_incomplete', 'list_response');
      if (all.length > 1000 || body.transporters.length > 10000)
        return fail('cortex_source_too_large', 'summaries');
      const names = new Map(
        body.transporters.map((t) => [
          t.transporterId,
          [t.firstName, t.lastName]
            .filter((n) => typeof n === 'string')
            .join(' ')
            .trim(),
        ]),
      );
      const selected =
        scope.provider === 'ALL_DRIVERS' ? all : all.filter((s) => s.companyId === scope.provider);
      const routes = selected
        .map((s) => {
          const driver = names.get(s.transporterId);
          if (!token(s.itineraryId) || !token(s.transporterId) || !driver)
            throw new Error('cortex_invalid_identity');
          const { meals, unreadable } = routeMeals(s.breaks);
          const item = {
            id: s.itineraryId,
            transporterId: s.transporterId,
            driver,
            route: s.routeCode || 'UNASSIGNED',
            routeComplete: s.executionStatus === 'COMPLETE',
            meals,
            ...(unreadable ? { unreadable } : {}),
          };
          // As meal.js versions a route: by its published facts only.
          return { ...item, revision: JSON.stringify(item) };
        })
        .sort((a, b) => a.id.localeCompare(b.id));
      if (new Set(routes.map((c) => c.id)).size !== routes.length)
        return fail('cortex_invalid_identity', 'identity');
      return { candidates: routes };
    } catch (error) {
      const allowed = [
        'cortex_content_incomplete',
        'cortex_invalid_meal_evidence',
        'cortex_invalid_identity',
      ];
      return allowed.includes(error.message)
        ? fail(error.message, error.reason ? `list_${error.reason}` : 'evidence')
        : fail('cortex_content_incomplete', 'script_error');
    }
  };
  // Waits for `input`'s route, or reads the response that already arrived for it.
  const wait = (input, kept) => {
    const id = input.candidate.id;
    results.delete(id);
    if (kept) results.set(id, read(kept.status, kept.text, input));
    else waiting.set(id, input);
  };
  Object.defineProperty(window, '__dispatchMeals', {
    value: Object.freeze({
      // The one route the next itinerary response is read for, forgetting any other.
      // Only Cortex's own documents are told, never a sign-in page the tab was sent to.
      expect(input) {
        if (location.origin !== input.origin) return false;
        const kept = unclaimed.get(input.candidate.id);
        waiting.clear();
        unclaimed.clear();
        wait(input, kept);
        return true;
      },
      // A route waited for beside the others.
      want(input) {
        if (location.origin !== input.origin) return false;
        const kept = unclaimed.get(input.candidate.id);
        unclaimed.delete(input.candidate.id);
        wait(input, kept);
        return true;
      },
      take(id) {
        const result = results.get(id) ?? null;
        results.delete(id);
        return result;
      },
      // Every route read so far, by id, each taken once.
      taken() {
        const all = Object.fromEntries(results);
        results.clear();
        return all;
      },
      // The list's routes once the application has the list, else null.
      list(input) {
        if (location.origin !== input.origin || !list) return null;
        return candidates(input.scope);
      },
    }),
  });
  const path = '/operations/execution/api/itineraries/';
  const summaries = '/operations/execution/api/summaries';
  const address = (url) => {
    try {
      const parsed = new URL(url, location.href);
      return parsed.origin === location.origin ? parsed : null;
    } catch {}
    return null;
  };
  const itinerary = (url) => {
    const parsed = address(url);
    if (parsed?.pathname.startsWith(path))
      return parsed.searchParams.get('itineraryId') ?? parsed.pathname.slice(path.length);
    return null;
  };
  const listed = (url) => address(url)?.pathname === summaries;
  // What arrived for route `id`: read now when it is waited for, kept otherwise.
  const arrived = (id, status, text) => {
    const input = waiting.get(id);
    if (input) {
      waiting.delete(id);
      results.set(id, read(status, text, input));
      return;
    }
    unclaimed.delete(id);
    unclaimed.set(id, { status, text });
    while (unclaimed.size > 4) unclaimed.delete(unclaimed.keys().next().value);
  };
  // The application fetches with XHR today; a fetch is read the same way.
  const fetched = window.fetch;
  window.fetch = async function (resource, init) {
    const url = resource instanceof Request ? resource.url : String(resource);
    const id = itinerary(url);
    const response = await fetched.call(this, resource, init);
    if (listed(url) && response.status === 200)
      list = { address: url, text: await response.clone().text() };
    if (!id) return response;
    if (response.status !== 200) {
      arrived(id, response.status, '');
      return response;
    }
    arrived(id, response.status, await response.text());
    return new Response('', { status: 503 });
  };
  const real = (name) => Object.getOwnPropertyDescriptor(XMLHttpRequest.prototype, name).get;
  const status = real('status'),
    text = real('responseText'),
    response = real('response');
  // A finished request's body as text, whatever type the application asked for.
  const body = (xhr) =>
    xhr.responseType === 'json' ? JSON.stringify(response.call(xhr)) : text.call(xhr);
  const open = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function (method, url, ...rest) {
    const id = itinerary(url);
    if (listed(url))
      this.addEventListener('readystatechange', () => {
        if (this.readyState === 4 && status.call(this) === 200)
          list = { address: String(url), text: body(this) };
      });
    if (id) {
      // Read once the response is complete, before the application sees any of it,
      // whichever of its handlers runs first; a successful response is then withheld.
      let done = false,
        withheld = false;
      const json = () => this.responseType === 'json';
      const take = () => {
        if (done || this.readyState !== 4) return;
        done = true;
        const code = status.call(this);
        withheld = code === 200;
        arrived(id, code, withheld ? body(this) : '');
      };
      const hidden = (value, empty) => ({
        configurable: true,
        get: () => (take(), withheld ? empty() : value.call(this)),
      });
      Object.defineProperties(this, {
        status: hidden(status, () => 0),
        responseText: hidden(text, () => ''),
        response: hidden(response, () => (json() ? null : '')),
      });
      this.addEventListener('readystatechange', take);
    }
    return open.call(this, method, url, ...rest);
  };
};
