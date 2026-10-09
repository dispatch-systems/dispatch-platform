// Select the four meal timestamps inside the page. No delivery history, package
// references, stop/task IDs, addresses, cookies or tokens leave the page; of the stops,
// only the places in the route of the two that bound each meal. Runs in the application's world
// because React's props are not visible from an isolated JavaScript world. `rules` is
// meal_rules.js. `input.kind` asks for the list's routes, a route's evidence, or only
// whether the page shows the scope's station, day and provider (`scope`).
(input, rules) => {
  const { fail, token, meals, detail } = rules;
  const scope = input.scope;
  const url = new URL(location.href);
  if (url.origin !== input.origin || url.username || url.password)
    return fail('cortex_scope_mismatch', 'origin');
  if (!url.pathname.startsWith('/operations/execution/itineraries'))
    return fail('cortex_content_incomplete', 'path');
  if (
    url.searchParams.get('selectedDay') !== scope.date ||
    url.searchParams.get('serviceAreaId') !== scope.serviceAreaId ||
    url.searchParams.get('provider') !== scope.provider
  )
    return fail('cortex_scope_mismatch', 'url_scope');
  let root;
  const seen = new Set();
  const elements = document.querySelectorAll('*');
  if (elements.length > 60000) return fail('cortex_source_too_large', 'elements');
  for (const element of elements) {
    const key = Object.keys(element).find((k) => k.startsWith('__reactFiber'));
    for (let fiber = element[key], depth = 0; fiber && depth < 80; fiber = fiber.return, depth++) {
      if (seen.has(fiber)) break;
      seen.add(fiber);
      const props = fiber.memoizedProps;
      if (
        props &&
        Array.isArray(props.allItinerarySummaries) &&
        props.transporterSummary &&
        (input.kind !== 'detail' || props.itineraryDetails)
      )
        root = props;
    }
  }
  if (
    !root ||
    root.isLoadingSummaries !== false ||
    (input.kind === 'detail' && root.isLoadingItineraryDetails !== false)
  )
    return fail('cortex_content_incomplete', 'summaries_loading');
  const station = root.selectedStation;
  if (
    !station ||
    root.selectedDay !== scope.date ||
    root.serviceAreaId !== scope.serviceAreaId ||
    station.serviceAreaID !== scope.serviceAreaId ||
    station.defaultStationCode !== scope.station ||
    root.providerFilterValue !== scope.provider ||
    !Array.isArray(root.providerFilterOptions) ||
    !root.providerFilterOptions.some((p) => p.value === scope.provider)
  )
    return fail('cortex_scope_mismatch', 'page_scope');
  try {
    const zone = (value) =>
      new Intl.DateTimeFormat('en-US', { timeZone: value }).resolvedOptions().timeZone;
    if (typeof station.timeZone !== 'string' || zone(station.timeZone) !== zone(scope.timezone))
      return fail('cortex_timezone_mismatch', 'timezone');
  } catch {
    return fail('cortex_timezone_mismatch', 'timezone');
  }
  if (input.kind === 'scope') return { scope: true };
  try {
    const all = root.allItinerarySummaries;
    if (all.length > 1000) return fail('cortex_source_too_large', 'summaries');
    // Use the full loaded list, independent of visual text/progress filters.
    const selected =
      scope.provider === 'ALL_DRIVERS' ? all : all.filter((s) => s.companyId === scope.provider);
    if (scope.provider === 'ALL_DSPS') return fail('invalid_cortex_scope', 'provider');
    const candidates = selected
      .map((s) => {
        const driver = root.transporterSummary[s.transporterId]?.transporterName;
        if (
          !token(s.itineraryId) ||
          !token(s.transporterId) ||
          typeof driver !== 'string' ||
          !driver.trim()
        )
          throw new Error('cortex_invalid_identity');
        const item = {
          id: s.itineraryId,
          transporterId: s.transporterId,
          driver,
          route: s.routeCode || 'UNASSIGNED',
          routeComplete: s.executionStatus === 'COMPLETE',
          meals: meals(s.breaks),
        };
        // Only published facts version a route. Delivery progress changes with
        // every package, so including it would fail every sync of a working day;
        // each detail read validates its own stops and deliveries instead.
        return { ...item, revision: JSON.stringify(item) };
      })
      .sort((a, b) => a.id.localeCompare(b.id));
    if (new Set(candidates.map((c) => c.id)).size !== candidates.length)
      return fail('cortex_invalid_identity', 'identity');
    if (input.kind === 'list') return { candidates };
    const c = input.candidate;
    const current = candidates.find((v) => v.id === c.id);
    if (!current || current.revision !== c.revision)
      return fail('cortex_source_changed', 'route_changed');
    return detail(root.itineraryDetails, c, scope, true);
  } catch (error) {
    const allowed = [
      'cortex_content_incomplete',
      'cortex_invalid_meal_evidence',
      'cortex_invalid_identity',
    ];
    // A rejected meal punch names its rule and whether the list or the route showed it.
    return allowed.includes(error.message)
      ? fail(error.message, error.reason ? `${input.kind}_${error.reason}` : 'evidence')
      : fail('cortex_content_incomplete', 'script_error');
  }
};
