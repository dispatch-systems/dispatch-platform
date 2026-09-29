// Cortex's execution pages as the meal collector meets them: one application whose list
// is in its React props, which fetches an itinerary's details from the itinerary API with
// XMLHttpRequest when it loads on, or moves to, that itinerary's address, and renders
// them with an id on each stop; the API sends none.
export const itineraryApi = '/operations/execution/api/itineraries/';

/** How the application misbehaves on one page. */
export interface Quirks {
  /** It shows this itinerary's details instead of the address's, as Cortex sometimes does. */
  show?: string;
  /** It ignores moves to these itineraries, fetching nothing. */
  still?: string[];
}

/** An execution page showing list props `props`. */
export function executionPage(props: object, quirks: Quirks = {}): string {
  return `<title>Delivery Execution</title><main id="application"></main><script>
    const list = ${JSON.stringify(props)};
    const quirks = ${JSON.stringify(quirks)};
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
        encodeURIComponent(quirks.show || id) + '&serviceAreaId=' + list.serviceAreaId);
      request.send();
    };
    addEventListener('popstate', () => show(true));
    show(false);
  </script>`;
}
