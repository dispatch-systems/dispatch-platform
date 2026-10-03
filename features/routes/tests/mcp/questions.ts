import {
  day,
  days,
  local,
  open,
  top,
  type Question,
  type World,
} from '../../../../core/mcp/tests/support/questions.js';

/** What agents are asked about routes, and the drivers asked about, whom other questions ask
 * about too. */
export function routesQuestions(world: World) {
  const routes = open(world, 'routedata');
  try {
    const { yesterday, weekFrom, lastSunday, lastSaturday, fortnight, week } = days(world);
    const all = (sql: string, ...params: (string | number)[]) =>
      routes.prepare(sql).all(...params) as Record<string, any>[];
    const itineraries = `FROM itineraries i JOIN route_publications p ON p.id=i.publication_id AND p.active=1`;
    const perDriver = (from: string, to: string, column: string) =>
      all(
        `SELECT i.driver_name name, SUM(i.${column}) value ${itineraries}
         WHERE i.day BETWEEN ? AND ? GROUP BY i.driver_name`,
        from,
        to,
      ) as { name: string; value: number }[];
    const packages = perDriver(world.from, world.to, 'delivered');
    // The driver with the most route days, and one in the middle of the table, to ask about.
    const busiest = all(
      `SELECT i.driver_name name, COUNT(*) n ${itineraries} GROUP BY i.driver_name ORDER BY n DESC, name`,
    )[0]!.name as string;
    const middle = [...packages].sort((a, b) => a.value - b.value)[5]!.name;
    const onDay = (name: string, date: string) =>
      all(`SELECT i.* ${itineraries} WHERE i.driver_name=? AND i.day=?`, name, date)[0];
    const workedDay =
      [yesterday, day(yesterday, -1), day(yesterday, -2)].find((d) => onDay(middle, d)) ??
      yesterday;
    const ride = onDay(middle, workedDay)!;
    const task = (state: string) =>
      routes
        .prepare(
          `SELECT t.tracking_id tracking, i.driver_name name, i.route_code route FROM tasks t
           JOIN route_publications p ON p.id=t.publication_id AND p.active=1
           JOIN itineraries i ON i.publication_id=t.publication_id AND i.itinerary_id=t.itinerary_id
           WHERE t.task_state=? AND t.task_type='DROP_OFF' ORDER BY t.day DESC, t.tracking_id LIMIT 1`,
        )
        .get(state) as { tracking: string; name: string; route: string };
    const delivered = task('DELIVERED');
    const returned = task('BACK_TO_ORIGIN');
    const team = (from: string, to: string, column: string) =>
      all(`SELECT SUM(i.${column}) n ${itineraries} WHERE i.day BETWEEN ? AND ?`, from, to)[0]!
        .n as number;
    const middleDays = all(
      `SELECT i.day, i.delivered value, i.completed_locations stops ${itineraries}
       WHERE i.driver_name=? AND i.day BETWEEN ? AND ? ORDER BY i.day`,
      middle,
      world.from,
      world.to,
    );
    const bestDay = Math.max(...middleDays.map((row) => row.value));
    const transporter = all(
      `SELECT DISTINCT transporter_id id ${itineraries} WHERE i.driver_name=?`,
      busiest,
    )[0]!.id as string;
    const ranked = [...packages].sort((a, b) => b.value - a.value);
    // Packages as Amazon recorded each drop-off: delivered, or brought back with a reason.
    const tasks = (where: string, ...params: (string | number)[]) =>
      (
        routes
          .prepare(
            `SELECT COUNT(*) n FROM tasks t
             JOIN route_publications p ON p.id=t.publication_id AND p.active=1
             JOIN itineraries i ON i.publication_id=t.publication_id AND i.itinerary_id=t.itinerary_id
             WHERE t.task_type='DROP_OFF' AND t.active=1 AND ${where}`,
          )
          .get(...params) as { n: number }
      ).n;
    const returner =
      (
        routes
          .prepare(
            `SELECT i.driver_name name FROM tasks t
             JOIN route_publications p ON p.id=t.publication_id AND p.active=1
             JOIN itineraries i ON i.publication_id=t.publication_id AND i.itinerary_id=t.itinerary_id
             WHERE t.task_type='DROP_OFF' AND t.active=1 AND t.task_state='BACK_TO_ORIGIN' AND t.day=?
             GROUP BY name ORDER BY COUNT(*) DESC, name LIMIT 1`,
          )
          .get(yesterday) as { name: string } | undefined
      )?.name ?? middle;
    const reasons = routes
      .prepare(
        `SELECT lower(t.state_context) reason, COUNT(*) n FROM tasks t
         JOIN route_publications p ON p.id=t.publication_id AND p.active=1
         WHERE t.task_type='DROP_OFF' AND t.active=1 AND t.task_state='BACK_TO_ORIGIN'
         AND t.day BETWEEN ? AND ? GROUP BY reason ORDER BY n DESC`,
      )
      .all(day(world.to, -13), world.to) as { reason: string; n: number }[];

    const questions: Question[] = [
      {
        id: 'driver_delivered_last_week',
        ask: `How many packages did ${middle} deliver last week?`,
        format: 'a whole number',
        grade: 'number',
        expected: [
          String(
            tasks(
              "t.task_state='DELIVERED' AND i.driver_name=? AND t.day BETWEEN ? AND ?",
              middle,
              lastSunday,
              lastSaturday,
            ),
          ),
        ],
      },
      {
        id: 'driver_returned_last_night',
        ask: `Did ${returner} return any packages last night? How many?`,
        format: 'a whole number, 0 if none',
        grade: 'number',
        expected: [
          String(
            tasks(
              "t.task_state='BACK_TO_ORIGIN' AND i.driver_name=? AND t.day=?",
              returner,
              yesterday,
            ),
          ),
        ],
      },
      {
        id: 'business_closed_last_night',
        ask: 'How many business-closed packages did we have last night?',
        format: 'a whole number, 0 if none',
        grade: 'number',
        expected: [
          String(
            tasks(
              "t.task_state='BACK_TO_ORIGIN' AND t.state_context='BUSINESS_CLOSED' AND t.day=?",
              yesterday,
            ),
          ),
        ],
      },
      {
        id: 'top_return_reason',
        ask: 'What was the most common reason packages were returned in the last 14 days?',
        format: "the reason as Amazon names it, such as 'business closed'",
        grade: 'text',
        expected: reasons.filter((r) => r.n === reasons[0]!.n).map((r) => r.reason),
      },
      {
        id: 'top_stops_yesterday',
        ask: 'Who completed the most delivery stops yesterday?',
        format: "the driver's full name",
        grade: 'name',
        expected: top(perDriver(yesterday, yesterday, 'completed_locations')),
      },
      {
        id: 'top_packages_fortnight',
        ask: `Who delivered the most packages ${fortnight}?`,
        format: "the driver's full name",
        grade: 'name',
        expected: top(packages),
      },
      {
        id: 'fewest_packages_week',
        ask: `Among drivers who drove at least one route ${week}, who delivered the fewest packages?`,
        format: "the driver's full name",
        grade: 'name',
        expected: top(perDriver(weekFrom, yesterday, 'delivered'), true),
      },
      {
        id: 'third_packages',
        ask: `Rank the drivers by packages delivered ${fortnight}. Who is third?`,
        format: "the driver's full name",
        grade: 'name',
        expected: ranked.filter((row) => row.value === ranked[2]!.value).map((row) => row.name),
      },
      {
        id: 'driver_packages',
        ask: `How many packages did ${middle} deliver ${fortnight}?`,
        format: 'a whole number',
        grade: 'number',
        expected: [String(packages.find((row) => row.name === middle)!.value)],
      },
      {
        id: 'routes_yesterday',
        ask: 'How many routes ran yesterday?',
        format: 'a whole number',
        grade: 'number',
        expected: [String(all(`SELECT COUNT(*) n ${itineraries} WHERE i.day=?`, yesterday)[0]!.n)],
      },
      {
        id: 'undeliverable_yesterday',
        ask: 'How many packages were undeliverable yesterday, across every route?',
        format: 'a whole number',
        grade: 'number',
        expected: [String(team(yesterday, yesterday, 'undeliverable') ?? 0)],
      },
      {
        id: 'route_code',
        ask: `Which route did ${middle} drive on ${workedDay}?`,
        format: 'the route code, such as CX101',
        grade: 'text',
        expected: [ride.route_code],
      },
      {
        id: 'departure',
        ask: `What time did ${middle} leave the station on ${workedDay}?`,
        format: "24-hour HH:MM in the DSP's local time",
        grade: 'text',
        expected: [local(ride.departed_at, world.timezone)],
      },
      {
        id: 'package_driver',
        ask: `Who delivered package ${delivered.tracking}?`,
        format: "the driver's full name",
        grade: 'name',
        expected: [delivered.name],
      },
      {
        id: 'package_outcome',
        ask: `What happened to package ${returned.tracking.toLowerCase()}?`,
        format: 'one word: delivered, returned or open',
        grade: 'text',
        expected: ['returned'],
      },
      {
        id: 'today_no_data',
        ask: 'How many delivery stops have been completed today?',
        format: "a whole number, or 'no data' if Dispatch has nothing for today",
        grade: 'text',
        expected: ['no data'],
      },
      {
        id: 'dispatcher_no_routes',
        ask: `How many delivery stops did Avery Morgan complete ${fortnight}?`,
        format: "a whole number, or 'none' if they drove no routes",
        grade: 'text',
        expected: ['none', '0'],
      },
      {
        id: 'transporter',
        ask: `Who is Amazon transporter ID ${transporter}?`,
        format: "the driver's full name",
        grade: 'name',
        expected: [busiest],
      },
      {
        id: 'average_stops',
        ask: `On average, how many delivery stops did ${middle} complete per route day ${fortnight}?`,
        format: 'a number to one decimal',
        grade: 'number',
        expected: [
          (middleDays.reduce((sum, row) => sum + row.stops, 0) / middleDays.length).toFixed(1),
        ],
        tolerance: 0.15,
      },
      {
        id: 'team_last_week',
        ask: 'How many packages did the whole team deliver last week?',
        format: 'a whole number',
        grade: 'number',
        expected: [String(team(lastSunday, lastSaturday, 'delivered') ?? 0)],
      },
      {
        id: 'best_day',
        ask: `On which day ${fortnight} did ${middle} deliver the most packages?`,
        format: 'the date as YYYY-MM-DD',
        grade: 'date',
        expected: middleDays.filter((row) => row.value === bestDay).map((row) => row.day),
      },
    ];
    return { busiest, middle, questions };
  } finally {
    routes.close();
  }
}

/** Where each package was delivered, as its drop-off recorded it. */
export function addresses(world: World, trackings: readonly string[]) {
  const routes = open(world, 'routedata');
  try {
    const addressOf = routes.prepare(
      `SELECT t.address_id FROM tasks t JOIN route_publications p ON p.id=t.publication_id AND p.active=1
       WHERE t.tracking_id=? AND t.task_type='DROP_OFF' AND t.active=1 LIMIT 1`,
    );
    return trackings.map(
      (tracking) => (addressOf.get(tracking) as { address_id: string } | undefined)?.address_id,
    );
  } finally {
    routes.close();
  }
}
