// Installed in the application's world before its scripts run. When the application
// fetches the itinerary the collection is waiting for, this reads that route's meal
// evidence from the response inside the page with meal.js's rules (`rules` is
// meal_rules.js), keeps only the result, and answers the application with a failed
// request, so the page never renders the itinerary. As with meal.js, no delivery history,
// package references, stop/task IDs, addresses, cookies or tokens leave the page: only the
// four meal timestamps, their coverage and the places in the route of the bounding stops.
(rules) => {
  if (window.__dispatchMeals) return;
  const { fail, detail } = rules;
  const results = new Map();
  let pending = null;
  // A response that arrived before its route was named, as on a freshly loaded page,
  // waits here until it is; it never leaves the page.
  const unclaimed = new Map();
  const read = (status, text, input) => {
    // The application answers a failed request itself; the collection loads the page.
    if (status !== 200) return { unanswered: true };
    try {
      return detail(JSON.parse(text).itineraryDetails, input.candidate, input.scope, false);
    } catch (error) {
      const allowed = ['cortex_content_incomplete', 'cortex_invalid_meal_evidence'];
      return allowed.includes(error.message)
        ? fail(error.message, 'evidence')
        : fail('cortex_content_incomplete', 'script_error');
    }
  };
  Object.defineProperty(window, '__dispatchMeals', {
    value: Object.freeze({
      // The route the next itinerary response is read for. Only Cortex's own documents
      // are told, never a sign-in page the tab was sent to.
      expect(input) {
        if (location.origin !== input.origin) return false;
        results.delete(input.candidate.id);
        const waiting = unclaimed.get(input.candidate.id);
        unclaimed.clear();
        pending = null;
        if (waiting) results.set(input.candidate.id, read(waiting.status, waiting.text, input));
        else pending = input;
        return true;
      },
      take(id) {
        const result = results.get(id) ?? null;
        results.delete(id);
        return result;
      },
    }),
  });
  const path = '/operations/execution/api/itineraries/';
  const itinerary = (url) => {
    try {
      const address = new URL(url, location.href);
      if (address.origin === location.origin && address.pathname.startsWith(path))
        return address.searchParams.get('itineraryId') ?? address.pathname.slice(path.length);
    } catch {}
    return null;
  };
  // What arrived for route `id`: read now when it is the route named, kept otherwise.
  const arrived = (id, status, text) => {
    if (pending && pending.candidate.id === id) {
      results.set(id, read(status, text, pending));
      pending = null;
    } else unclaimed.set(id, { status, text });
  };
  // The application fetches with XHR today; a fetch is read the same way.
  const fetched = window.fetch;
  window.fetch = async function (resource, init) {
    const id = itinerary(resource instanceof Request ? resource.url : String(resource));
    const response = await fetched.call(this, resource, init);
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
  const open = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function (method, url, ...rest) {
    const id = itinerary(url);
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
        const body = !withheld
          ? ''
          : json()
            ? JSON.stringify(response.call(this))
            : text.call(this);
        arrived(id, code, body);
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
