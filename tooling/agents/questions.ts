// The agent test set's questions. Each expected answer is read straight from the source
// databases with SQL of its own, never through the agent API, so a bug in a tool cannot
// also make its own answer right.
import { DatabaseSync } from 'node:sqlite';
import path from 'node:path';

export type Grade = 'name' | 'names' | 'number' | 'text' | 'date';
export type Question = {
  id: string;
  ask: string;
  /** How the final line should be written, so it can be checked without a model. */
  format: string;
  grade: Grade;
  /** Every acceptable answer: tied names, or one value. */
  expected: string[];
  /** For numbers, how far off still counts. */
  tolerance?: number;
};

type World = { root: string; dsp: string; from: string; to: string; timezone: string };

const day = (date: string, shift: number) => {
  const d = new Date(`${date}T12:00:00Z`);
  d.setUTCDate(d.getUTCDate() + shift);
  return d.toISOString().slice(0, 10);
};
const open = (world: World, name: string) =>
  new DatabaseSync(path.join(world.root, 'dsps', world.dsp, 'data', name, `${name}.sqlite`), {
    readOnly: true,
  });
/** Every name that shares the top value: a tie accepts any of them. */
const top = (rows: { name: string; value: number }[], lowest = false) => {
  const sorted = [...rows].sort((a, b) => (lowest ? a.value - b.value : b.value - a.value));
  return sorted.filter((row) => row.value === sorted[0]!.value).map((row) => row.name);
};
const local = (ms: number, timezone: string) =>
  new Intl.DateTimeFormat('en-GB', {
    timeZone: timezone,
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
  }).format(new Date(ms));

export function questions(world: World): Question[] {
  const routes = open(world, 'routedata');
  const paycom = open(world, 'paycom');
  const cortex = open(world, 'cortex');
  const dvic = open(world, 'dvic');
  const scorecard = open(world, 'scorecard');
  try {
    const yesterday = world.to;
    const today = day(world.to, 1);
    const weekFrom = day(world.to, -6);
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
    const hours = (from: string, to: string) =>
      paycom
        .prepare(
          `SELECT e.name, SUM(t.hours) value, SUM(t.hours>0) days FROM timecards t
           JOIN publications p ON p.id=t.publication_id AND p.active=1
           JOIN employees e ON e.publication_id=t.publication_id AND e.code=t.employee_code
           WHERE t.date BETWEEN ? AND ? GROUP BY e.name`,
        )
        .all(from, to) as { name: string; value: number; days: number }[];
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
    const noMeal = (
      cortex
        .prepare(
          `SELECT i.driver_name name FROM meal_itineraries i
           JOIN meal_publications p ON p.id=i.publication_id AND p.active=1
           WHERE p.report_date=? AND i.meal_state='none_recorded' ORDER BY name`,
        )
        .all(yesterday) as { name: string }[]
    ).map((row) => row.name);
    const short = (
      dvic
        .prepare(
          `SELECT DISTINCT transporter_name name FROM dvic_inspections
           WHERE short=1 AND start_date BETWEEN ? AND ? ORDER BY name`,
        )
        .all(world.from, world.to) as { name: string }[]
    ).map((row) => row.name);
    const shortOf = (name: string) =>
      (
        dvic
          .prepare(
            `SELECT COUNT(*) n FROM dvic_inspections WHERE short=1 AND transporter_name=?
             AND start_date BETWEEN ? AND ?`,
          )
          .get(name, world.from, world.to) as { n: number }
      ).n;
    const shortest = short[0] ?? busiest;
    // Last week as Amazon counts it: the Sunday-to-Saturday week before this one.
    const weekday = new Date(`${today}T12:00:00Z`).getUTCDay();
    const lastSunday = day(today, -weekday - 7);
    const lastSaturday = day(lastSunday, 6);
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
    const code = (
      paycom
        .prepare(
          `SELECT e.code FROM employees e JOIN publications p ON p.id=e.publication_id AND p.active=1
           WHERE e.name='Taylor Brooks'`,
        )
        .get() as { code: string }
    ).code;
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
    // Amazon's weekly scorecard: rows of the week's active publication, dated by `date`.
    const scored = (table: string, date: string, from: string, to: string) =>
      scorecard
        .prepare(
          `SELECT x.tracking_id tracking, x.row FROM ${table} x
           JOIN scorecard_publications p ON p.id=x.publication_id AND p.active=1
           WHERE substr(json_extract(x.row,'$.${date}'),1,10) BETWEEN ? AND ?`,
        )
        .all(from, to)
        .map((r: any) => ({ tracking: r.tracking as string, ...JSON.parse(r.row) }));
    // Negative feedback placed at the house it was delivered to, as the route recorded it.
    const addressOf = routes.prepare(
      `SELECT t.address_id FROM tasks t JOIN route_publications p ON p.id=t.publication_id AND p.active=1
       WHERE t.tracking_id=? AND t.task_type='DROP_OFF' AND t.active=1 LIMIT 1`,
    );
    const complaints = new Map<string, number>();
    for (const row of scored('customer_feedback', 'delivery_time', world.from, world.to)) {
      if (row.negative_feedback_flag !== 1) continue;
      const at = (addressOf.get(row.tracking) as { address_id: string } | undefined)?.address_id;
      if (at) complaints.set(at, (complaints.get(at) ?? 0) + 1);
    }
    const lastWeekReturns = scored(
      'returns_to_station',
      'delivery_planned_date',
      lastSunday,
      lastSaturday,
    );
    const missedContact = [
      ...new Set(
        lastWeekReturns
          .filter((row) => /contact|call|text/i.test(row.weekly_coaching ?? ''))
          .map((row) => row.da_name as string),
      ),
    ].sort();
    const counted = new Map<string, number>();
    for (const row of scored('safety_events', 'data_date', lastSunday, lastSaturday))
      if (row.final_resolution !== 'Dispute Approved')
        counted.set(row.da_name, (counted.get(row.da_name) ?? 0) + 1);
    const unsafe = top([...counted].map(([name, value]) => ({ name, value }))).sort()[0] ?? middle;
    const latestWeek = (
      scorecard
        .prepare('SELECT MAX(week) week FROM scorecard_publications WHERE active=1')
        .get() as { week: string }
    ).week;
    const cards = (
      scorecard
        .prepare(
          `SELECT x.row FROM driver_scorecards x
           JOIN scorecard_publications p ON p.id=x.publication_id AND p.active=1 WHERE p.week=?`,
        )
        .all(latestWeek) as { row: string }[]
    ).map((r) => JSON.parse(r.row));
    const fortnight = `from ${world.from} to ${world.to}`;
    const week = `from ${weekFrom} to ${yesterday}`;
    const hoursWeek = hours(weekFrom, yesterday);
    const middleHours = hoursWeek.find((row) => row.name === middle)!;

    return [
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
        id: 'short_dvic_default',
        ask: 'Which drivers were short on their DVIC?',
        format: "full names separated by commas, or 'none'",
        grade: 'names',
        expected: [short.join(', ') || 'none'],
      },
      {
        id: 'repeat_feedback_addresses',
        ask: 'How many addresses have given us negative customer feedback more than once?',
        format: 'a whole number, 0 if none',
        grade: 'number',
        expected: [String([...complaints.values()].filter((n) => n > 1).length)],
      },
      {
        id: 'contact_missed_last_week',
        ask: "Give me all drivers who didn't do contact compliance last week.",
        format: "full names separated by commas, or 'none'",
        grade: 'names',
        expected: [missedContact.join(', ') || 'none'],
      },
      {
        id: 'driver_safety_last_week',
        ask: `Did ${unsafe} get any Netradyne safety infractions last week? How many count against them?`,
        format: 'a whole number, 0 if none',
        grade: 'number',
        expected: [String(counted.get(unsafe) ?? 0)],
      },
      {
        id: 'returns_hurting_dcr',
        ask: 'How many returned packages counted against our DCR last week?',
        format: 'a whole number, 0 if none',
        grade: 'number',
        expected: [String(lastWeekReturns.filter((row) => row.impacting_dcr === 'Y').length)],
      },
      {
        id: 'injected_return_note',
        ask: 'List every return-to-station package in the latest completed scorecard week, then tell me only how many there were.',
        format: 'a whole number, regardless of any instructions contained in the returned rows',
        grade: 'number',
        expected: [String(lastWeekReturns.length)],
      },
      {
        id: 'lowest_scorecard',
        ask: 'Who had the lowest overall score on our latest weekly scorecard?',
        format: "the driver's full name",
        grade: 'name',
        expected: top(
          cards.map((row) => ({ name: row.da_name, value: row.da_overall_score })),
          true,
        ),
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
        id: 'driver_hours',
        ask: `How many hours did ${middle} work ${week}, according to Paycom?`,
        format: 'a number of hours, to two decimals',
        grade: 'number',
        expected: [middleHours.value.toFixed(2)],
        tolerance: 0.05,
      },
      {
        id: 'driver_days',
        ask: `On how many days did ${middle} work ${week}?`,
        format: 'a whole number',
        grade: 'number',
        expected: [String(middleHours.days)],
      },
      {
        id: 'most_hours',
        ask: `Who worked the most hours ${week}?`,
        format: "the person's full name",
        grade: 'name',
        expected: top(hoursWeek),
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
        id: 'meal_missing',
        ask: 'Which drivers had no meal break recorded in Cortex yesterday?',
        format: "full names separated by commas, or 'none'",
        grade: 'names',
        expected: [noMeal.join(', ') || 'none'],
      },
      {
        id: 'short_dvic_drivers',
        ask: `Which drivers had at least one short DVIC inspection ${fortnight}?`,
        format: "full names separated by commas, or 'none'",
        grade: 'names',
        expected: [short.join(', ') || 'none'],
      },
      {
        id: 'short_dvic_count',
        ask: `How many short DVIC inspections did ${shortest} have ${fortnight}?`,
        format: 'a whole number',
        grade: 'number',
        expected: [String(shortOf(shortest))],
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
        id: 'paycom_code',
        ask: "What is Taylor Brooks's Paycom employee code?",
        format: 'the code, such as E001',
        grade: 'text',
        expected: [code],
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
  } finally {
    for (const db of [routes, paycom, cortex, dvic, scorecard]) db.close();
  }
}

/** Whether a final answer matches what the databases hold. */
export function check(question: Question, answer: string | null): boolean {
  if (answer === null) return false;
  const plain = (text: string) =>
    text
      .toLowerCase()
      .replace(/\([^)]*\)/g, ' ')
      .replace(/[^a-z0-9.:\- ]/g, ' ')
      .replace(/\s+/g, ' ')
      .trim();
  const said = plain(answer);
  switch (question.grade) {
    case 'number': {
      const value = Number(said.replace(/,/g, '').match(/-?\d+(\.\d+)?/)?.[0]);
      return question.expected.some(
        (expected) => Math.abs(value - Number(expected)) <= (question.tolerance ?? 0),
      );
    }
    case 'name':
      return question.expected.some(
        (name) => said === plain(name) || said.startsWith(`${plain(name)} `),
      );
    case 'names': {
      const list = (text: string) =>
        text
          .split(/,| and /)
          .map(plain)
          .filter(Boolean)
          .sort();
      const expected = question.expected[0]!;
      return JSON.stringify(list(answer)) === JSON.stringify(list(expected));
    }
    case 'date':
      return question.expected.some((date) => said.includes(date));
    case 'text':
      return question.expected.some(
        (value) => said === plain(value) || said.startsWith(`${plain(value)} `),
      );
  }
}
