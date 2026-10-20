// Cortex's execution pages as the meal collector meets them: one application that fetches
// its day's list from the summaries API and keeps it in its React props, and fetches an
// itinerary's details from the itinerary API with XMLHttpRequest when it loads on, or
// moves to, that itinerary's address, or when its store's API action is asked for it, and
// renders them with an id on each stop; the API sends none. Every move and every ask is
// fetched, even while an earlier one is still loading. A request its store's action sent
// carries `fixtureAsked`.
export const itineraryApi = '/operations/execution/api/itineraries/';
export const summariesApi = '/operations/execution/api/summaries';

/** How the application misbehaves on one page. */
export interface Quirks {
  /** It shows this itinerary's details instead of the address's, as Cortex sometimes does. */
  show?: string;
  /** It ignores moves to these itineraries, and asks of its API action for them, fetching
   * nothing. */
  still?: string[];
  /** Its React tree is given no store, so nothing can be asked of its API action. */
  storeless?: boolean;
}

/** A day's list as the summaries API sends it, each driver named `names[id]`, or for
 * their route. */
export function listResponse(
  routes: { transporterId: string; routeCode: string }[],
  names: Record<string, string> = {},
) {
  const transporters = new Map(
    routes.map((r) => {
      const [firstName, ...last] = (names[r.transporterId] ?? `Driver ${r.routeCode}`).split(' ');
      return [
        r.transporterId,
        { transporterId: r.transporterId, firstName, lastName: last.join(' ') },
      ];
    }),
  );
  return {
    itinerarySummaries: routes,
    transporters: [...transporters.values()],
    companies: [{ companyId: 'provider-1' }],
  };
}

/** An execution page at station TST1 (service area area-1), filtered to the address's
 * provider, its props beside the list's replaced by `props`. */
export function executionPage(quirks: Quirks = {}, props: object = {}): string {
  return `<title>Delivery Execution</title><main id="application"></main><script>
    const quirks = ${JSON.stringify(quirks)};
    const address = new URL(location.href).searchParams;
    const day = address.get('selectedDay');
    let list = {
      selectedDay: day,
      serviceAreaId: 'area-1',
      selectedStation: { serviceAreaID: 'area-1', defaultStationCode: 'TST1', timeZone: 'US/Pacific' },
      providerFilterValue: address.get('provider') ?? 'ALL_DSPS',
      providerFilterOptions: [{ value: 'provider-1' }],
      isLoadingSummaries: true,
      ...${JSON.stringify(props)},
    };
    const fiber = { memoizedProps: list };
    document.querySelector('main').__reactFiber$fixture = fiber;
    // The itinerary's details, fetched and shown; \`asked\` marks a request the action sent.
    const fetchDetails = (id, asked) => new Promise((resolve, reject) => {
      fiber.memoizedProps = { ...list, isLoadingItineraryDetails: true };
      const request = new XMLHttpRequest();
      request.onreadystatechange = () => {
        if (request.readyState !== 4) return;
        const details = request.status === 200 ? JSON.parse(request.responseText).itineraryDetails : null;
        if (details) details.stops = details.stops.map((stop, i) => ({ stopId: 'stop-' + (i + 1), ...stop }));
        fiber.memoizedProps = { ...list, isLoadingItineraryDetails: false, itineraryDetails: details };
        (request.status === 200 ? resolve : reject)(details);
      };
      request.open('GET', '${itineraryApi}?documentType=Itinerary&historicalDay=true&itineraryId=' +
        encodeURIComponent(quirks.show || id) + '&serviceAreaId=' + encodeURIComponent(list.serviceAreaId) +
        (asked ? '&fixtureAsked=1' : ''));
      request.send();
    });
    const store = { getState: () => ({}), subscribe: () => () => {}, dispatch: (action) => {
      const call = action['Call API'];
      if (!call) return action;
      const id = call.params.itineraryId;
      if (!call.endpoint.startsWith('${itineraryApi}') || (quirks.still || []).includes(id))
        return new Promise(() => {});
      return fetchDetails(id, true);
    } };
    if (!quirks.storeless) fiber.return = { memoizedProps: { store }, return: null };
    const show = (moved) => {
      const match = location.pathname.match(/^\\/operations\\/execution\\/itineraries\\/([^/]+)\\/documentType\\//);
      if (!match) return void (fiber.memoizedProps = list);
      const id = decodeURIComponent(match[1]);
      if (moved && (quirks.still || []).includes(id)) return;
      fetchDetails(id, false).catch(() => {});
    };
    addEventListener('popstate', () => show(true));
    const summaries = new XMLHttpRequest();
    summaries.onreadystatechange = () => {
      if (summaries.readyState !== 4 || summaries.status !== 200) return;
      const body = JSON.parse(summaries.responseText);
      list = {
        ...list,
        isLoadingSummaries: false,
        allItinerarySummaries: body.itinerarySummaries,
        transporterSummary: Object.fromEntries(body.transporters.map((t) =>
          [t.transporterId, { transporterName: t.firstName + ' ' + t.lastName }])),
      };
      show(false);
    };
    summaries.open('GET', '${summariesApi}?historicalDay=false&localDate=' + day + '&serviceAreaId=' + encodeURIComponent(list.serviceAreaId));
    summaries.send();
  </script>`;
}
