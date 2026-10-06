// The agent test set's questions that join two features' data: about a driver one feature
// names, in another's data.
import { days, top, type Question, type World } from '../../../core/mcp/tests/support/questions.js';
import { dvicQuestions, shortInspections } from '../../../features/dvic/tests/mcp/questions.js';
import { addresses, routesQuestions } from '../../../features/routes/tests/mcp/questions.js';
import { weeklyScorecardQuestions } from '../../../features/weekly_scorecard/tests/mcp/questions.js';
import { timecardQuestions } from '../../../features/timecard/tests/mcp/questions.js';

export function joinedQuestions(
  world: World,
  {
    routes,
    timecard,
    dvic,
    weeklyScorecard,
  }: {
    routes: ReturnType<typeof routesQuestions>;
    timecard: ReturnType<typeof timecardQuestions>;
    dvic: ReturnType<typeof dvicQuestions>;
    weeklyScorecard: ReturnType<typeof weeklyScorecardQuestions>;
  },
): Question[] {
  const { busiest, middle } = routes;
  const { fortnight, week } = days(world);
  // Negative feedback placed at the house it was delivered to, as the route recorded it.
  const complaints = new Map<string, number>();
  const negative = weeklyScorecard.feedback.filter((row) => row.negative_feedback_flag === 1);
  const places = addresses(
    world,
    negative.map((row) => row.tracking),
  );
  for (const at of places) if (at) complaints.set(at, (complaints.get(at) ?? 0) + 1);
  const { counted } = weeklyScorecard;
  const unsafe = top([...counted].map(([name, value]) => ({ name, value }))).sort()[0] ?? middle;
  const middleHours = timecard.hoursWeek.find((row) => row.name === middle)!;
  const shortest = dvic.short[0] ?? busiest;
  return [
    {
      id: 'repeat_feedback_addresses',
      ask: 'How many addresses have given us negative customer feedback more than once?',
      format: 'a whole number, 0 if none',
      grade: 'number',
      expected: [String([...complaints.values()].filter((n) => n > 1).length)],
    },
    {
      id: 'driver_safety_last_week',
      ask: `Did ${unsafe} get any Netradyne safety infractions last week? How many count against them?`,
      format: 'a whole number, 0 if none',
      grade: 'number',
      expected: [String(counted.get(unsafe) ?? 0)],
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
      id: 'short_dvic_count',
      ask: `How many short DVIC inspections did ${shortest} have ${fortnight}?`,
      format: 'a whole number',
      grade: 'number',
      expected: [String(shortInspections(world, shortest))],
    },
  ];
}
