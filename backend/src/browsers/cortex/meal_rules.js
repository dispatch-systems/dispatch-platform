// The meal rules meal.js and meal_hook.js share: how a route's meal breaks are read, and
// which of its deliveries bound each one. Both run them inside the page; only the four
// meal timestamps and their coverage leave it.
(() => {
  // `reason` is a fixed diagnostic label for job metrics, never page content.
  const fail = (error, reason) => ({ error, reason });
  const token = (value) => typeof value === 'string' && /^[A-Za-z0-9_.:#-]{1,256}$/.test(value);
  const stamp = (value) =>
    typeof value === 'number' && Number.isFinite(value) && value > 0
      ? Math.round(value < 100000000000 ? value * 1000 : value)
      : null;
  const meals = (values) => {
    if (!Array.isArray(values)) throw new Error('cortex_content_incomplete');
    const records = new Map();
    for (const b of values.filter((b) => b.type === 'MEAL')) {
      const start = stamp(b.timeStampOn),
        end = stamp(b.timeStampOff),
        id = b.breakId;
      if (
        !token(id) ||
        !token(b.punchId) ||
        !start ||
        !['ON', 'OFF'].includes(b.state) ||
        (b.state === 'OFF' && (!end || end < start)) ||
        (b.state === 'ON' && end !== null)
      )
        throw new Error('cortex_invalid_meal_evidence');
      const current = { id, start, end, sequence: b.sequenceNumber };
      const prior = records.get(id);
      if (prior) {
        if (!Number.isInteger(current.sequence) || prior.sequence !== current.sequence)
          throw new Error('cortex_invalid_meal_evidence');
        if ((prior.end === null) === (end === null)) {
          if (prior.start !== start || prior.end !== end)
            throw new Error('cortex_invalid_meal_evidence');
          continue;
        }
        // One logical break can retain its ON punch alongside the completed
        // OFF record. The completed pair is authoritative only for that same
        // break/sequence and an ON punch inside its recorded interval.
        const completed = end === null ? prior : current;
        const opened = end === null ? current : prior;
        if (opened.start < completed.start || opened.start > completed.end)
          throw new Error('cortex_invalid_meal_evidence');
        records.set(id, completed);
      } else records.set(id, current);
    }
    return [...records.values()]
      .map(({ id, start, end }) => ({ id, start, end }))
      .sort((a, b) => a.start - b.start || a.id.localeCompare(b.id));
  };
  // Candidate `c`'s evidence from its itinerary details `d`, as the page `rendered` them
  // or as the itinerary response sent them. The page gives each stop an id; the response
  // sends none, so only each stop's tasks are checked there.
  const detail = (d, c, scope, rendered) => {
    const localDate = Array.isArray(d.localDate)
      ? d.localDate.map((v, i) => String(v).padStart(i ? 2 : 4, '0')).join('-')
      : d.localDate;
    if (d.itineraryId !== c.id) return fail('cortex_scope_mismatch', 'detail_itinerary');
    // The page's own list was checked against `c` first, so another driver there is a
    // page yet to catch up; in a response, the route changed hands after the list was read.
    if (d.transporterId !== c.transporterId)
      return rendered
        ? fail('cortex_scope_mismatch', 'detail_transporter')
        : fail('cortex_source_changed', 'route_changed');
    if (d.serviceAreaId !== scope.serviceAreaId)
      return fail('cortex_scope_mismatch', 'detail_area');
    if (localDate !== scope.date) return fail('cortex_scope_mismatch', 'detail_date');
    const breaks = meals(d.breaks);
    if (
      JSON.stringify(breaks.map((m) => [m.id, m.start, m.end])) !==
        JSON.stringify(c.meals.map((m) => [m.id, m.start, m.end])) ||
      (d.executionStatus === 'COMPLETE') !== c.routeComplete
    )
      return fail('cortex_source_changed', 'meal_changed');
    if (
      !Array.isArray(d.stops) ||
      !Array.isArray(d.unknownStops) ||
      !Array.isArray(d.inactiveTasks) ||
      d.stops.length > 2000 ||
      d.inactiveTasks.length > 10000
    )
      return fail('cortex_content_incomplete', 'stops');
    // unknownStops records unplanned dwell locations (enter/exit coordinates
    // and times), not missing delivery tasks. It does not invalidate the
    // independently counted stops[].tasks delivery evidence.
    let complete =
      Number.isInteger(d.stopProgress?.total) && d.stops.length === d.stopProgress.total;
    const observations = new Map(),
      stopIds = new Set(),
      deliveries = new Map();
    let taskCount = 0;
    for (const stop of d.stops) {
      if (
        (rendered && (!token(stop.stopId) || stopIds.has(stop.stopId))) ||
        !Array.isArray(stop.tasks)
      )
        return fail('cortex_content_incomplete', 'stops');
      stopIds.add(stop.stopId);
      for (const task of stop.tasks) {
        if (++taskCount > 10000) return fail('cortex_source_too_large', 'tasks');
        if (!token(task.taskId)) return fail('cortex_content_incomplete', 'tasks');
        const time = stamp(task.actualExecutionTime ?? task.taskExecutionTime);
        const evidence = JSON.stringify([
          task.taskType,
          task.taskState,
          task.executionStatus,
          time,
          task.transporterId ?? null,
        ]);
        // Amazon can repeat a task across overlapping stop groups. Identical
        // facts represent one event; conflicting copies cannot establish gaps.
        if (observations.has(task.taskId)) {
          if (observations.get(task.taskId) !== evidence) {
            complete = false;
            deliveries.delete(task.taskId);
          }
          continue;
        }
        observations.set(task.taskId, evidence);
        if (task.taskState !== 'DELIVERED') continue;
        if (
          task.taskType !== 'DROP_OFF' ||
          task.executionStatus !== 'COMPLETE' ||
          !time ||
          (task.transporterId != null && task.transporterId !== c.transporterId)
        ) {
          complete = false;
          continue;
        }
        deliveries.set(task.taskId, time);
      }
    }
    const selectedMeals = breaks.map((meal) => {
      let lastDelivery = null,
        firstDelivery = null;
      if (complete) {
        for (const time of deliveries.values()) {
          // Package completion times matter, including different completions
          // within a group stop: latest before OUT, earliest after IN.
          if (time <= meal.start && (lastDelivery === null || time > lastDelivery))
            lastDelivery = time;
          if (
            meal.end !== null &&
            time >= meal.end &&
            (firstDelivery === null || time < firstDelivery)
          )
            firstDelivery = time;
        }
      }
      return { ...meal, lastDelivery, firstDelivery };
    });
    // Removed tasks have uncertain ownership. Their times can only invalidate
    // a boundary if they could be closer than the selected active delivery.
    // Never substitute them for this driver's delivery or discard a verified
    // boundary because an unrelated removed task occurred hours away.
    for (const task of d.inactiveTasks.filter((t) => t.taskState === 'DELIVERED')) {
      const time = stamp(task.actualExecutionTime ?? task.taskExecutionTime);
      if (
        selectedMeals.some(
          (meal) =>
            !time ||
            task.taskType !== 'DROP_OFF' ||
            task.executionStatus !== 'COMPLETE' ||
            (time <= meal.start && (meal.lastDelivery === null || time > meal.lastDelivery)) ||
            (meal.end !== null &&
              time >= meal.end &&
              (meal.firstDelivery === null || time < meal.firstDelivery)),
        )
      )
        complete = false;
    }
    if (!complete) {
      for (const meal of selectedMeals) {
        meal.lastDelivery = null;
        meal.firstDelivery = null;
      }
    }
    return {
      itinerary: {
        id: c.id,
        transporterId: c.transporterId,
        driver: c.driver,
        route: c.route,
        observedAt: Date.now(),
        routeComplete: c.routeComplete,
        deliveryCoverage: complete ? 'complete' : 'unavailable',
        meals: selectedMeals,
      },
    };
  };
  return { fail, token, stamp, meals, detail };
})();
