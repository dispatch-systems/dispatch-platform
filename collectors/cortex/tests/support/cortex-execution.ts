// Cortex's execution pages as the meal collector meets them: one application that fetches
// its day's list from the summaries API and keeps it in its React props, and fetches an
// itinerary's details from the itinerary API with XMLHttpRequest when it loads on, or
// moves to, that itinerary's address, and renders them with an id on each stop; the API
// sends none. Every move is fetched, even while an earlier one is still loading.
export const itineraryApi = '/operations/execution/api/itineraries/';
export const summariesApi = '/operations/execution/api/summaries';

/** How the application misbehaves on one page. */
export interface Quirks {
  /** It shows this itinerary's details instead of the address's, as Cortex sometimes does. */
  show?: string;
  /** It ignores moves to these itineraries, fetching nothing. */
  still?: string[];
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
    const show = (moved) => {
      const match = location.pathname.match(/^\\/operations\\/execution\\/itineraries\\/([^/]+)\\/documentType\\//);
      if (!match) return void (fiber.memoizedProps = list);
      const id = decodeURIComponent(match[1]);
      if (moved && (quirks.still || []).includes(id)) return;
      fiber.memoizedProps = { ...list, isLoadingItineraryDetails: true };
      const request = new XMLHttpRequest();
      request.onreadystatechange = () => {
        if (request.readyState !== 4) return;
        const details = request.status === 200 ? JSON.parse(request.responseText).itineraryDetails : null;
        if (details) details.stops = details.stops.map((stop, i) => ({ stopId: 'stop-' + (i + 1), ...stop }));
        fiber.memoizedProps = { ...list, isLoadingItineraryDetails: false, itineraryDetails: details };
      };
      request.open('GET', '${itineraryApi}?documentType=Itinerary&historicalDay=true&itineraryId=' +
        encodeURIComponent(quirks.show || id) + '&serviceAreaId=area-1');
      request.send();
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
    summaries.open('GET', '${summariesApi}?historicalDay=false&localDate=' + day + '&serviceAreaId=area-1');
    summaries.send();
  </script>`;
}
