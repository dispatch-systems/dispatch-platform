// The agent test set's questions: each feature's own, in its tests/mcp/, and those joining two
// features' data, in app/tests/mcp/, asked in one fixed order.
import type { Question, World } from '../../core/mcp/tests/support/questions.js';
import { joinedQuestions } from '../../app/tests/mcp/questions.js';
import { dvicQuestions } from '../../features/dvic/tests/mcp/questions.js';
import { routesQuestions } from '../../features/routes/tests/mcp/questions.js';
import { weeklyScorecardQuestions } from '../../features/weekly_scorecard/tests/mcp/questions.js';
import { timecardQuestions } from '../../features/timecard/tests/mcp/questions.js';

export type { Question } from '../../core/mcp/tests/support/questions.js';

/** Every question's id, in the order the test set asks them. */
const order = [
  'driver_delivered_last_week',
  'driver_returned_last_night',
  'business_closed_last_night',
  'top_return_reason',
  'short_dvic_default',
  'repeat_feedback_addresses',
  'contact_missed_last_week',
  'driver_safety_last_week',
  'returns_hurting_dcr',
  'injected_return_note',
  'lowest_scorecard',
  'top_stops_yesterday',
  'top_packages_fortnight',
  'fewest_packages_week',
  'third_packages',
  'driver_packages',
  'driver_hours',
  'driver_days',
  'most_hours',
  'routes_yesterday',
  'undeliverable_yesterday',
  'route_code',
  'departure',
  'package_driver',
  'package_outcome',
  'meal_missing',
  'short_dvic_drivers',
  'short_dvic_count',
  'today_no_data',
  'dispatcher_no_routes',
  'paycom_code',
  'transporter',
  'average_stops',
  'team_last_week',
  'best_day',
];

export function questions(world: World): Question[] {
  const routes = routesQuestions(world);
  const timecard = timecardQuestions(world);
  const dvic = dvicQuestions(world);
  const weeklyScorecard = weeklyScorecardQuestions(world);
  const all = [
    ...routes.questions,
    ...timecard.questions,
    ...dvic.questions,
    ...weeklyScorecard.questions,
    ...joinedQuestions(world, { routes, timecard, dvic, weeklyScorecard }),
  ];
  const ids = all.map((question) => question.id);
  if (ids.length !== order.length || !order.every((id) => ids.includes(id)))
    throw new Error(`the test set's order and its questions disagree: ${ids.join(', ')}`);
  return order.map((id) => all.find((question) => question.id === id)!);
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
