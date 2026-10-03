import {
  days,
  open,
  top,
  type Question,
  type World,
} from '../../../../core/mcp/tests/support/questions.js';

/** What agents are asked about timecards and meal breaks, and the week's hours, which other
 * questions ask about too. */
export function timecardQuestions(world: World) {
  const paycom = open(world, 'paycom');
  const cortex = open(world, 'cortex');
  try {
    const { yesterday, weekFrom, week } = days(world);
    const hours = (from: string, to: string) =>
      paycom
        .prepare(
          `SELECT e.name, SUM(t.hours) value, SUM(t.hours>0) days FROM timecards t
           JOIN publications p ON p.id=t.publication_id AND p.active=1
           JOIN employees e ON e.publication_id=t.publication_id AND e.code=t.employee_code
           WHERE t.date BETWEEN ? AND ? GROUP BY e.name`,
        )
        .all(from, to) as { name: string; value: number; days: number }[];
    const noMeal = (
      cortex
        .prepare(
          `SELECT i.driver_name name FROM meal_itineraries i
           JOIN meal_publications p ON p.id=i.publication_id AND p.active=1
           WHERE p.report_date=? AND i.meal_state='none_recorded' ORDER BY name`,
        )
        .all(yesterday) as { name: string }[]
    ).map((row) => row.name);
    const code = (
      paycom
        .prepare(
          `SELECT e.code FROM employees e JOIN publications p ON p.id=e.publication_id AND p.active=1
           WHERE e.name='Taylor Brooks'`,
        )
        .get() as { code: string }
    ).code;
    const hoursWeek = hours(weekFrom, yesterday);

    const questions: Question[] = [
      {
        id: 'most_hours',
        ask: `Who worked the most hours ${week}?`,
        format: "the person's full name",
        grade: 'name',
        expected: top(hoursWeek),
      },
      {
        id: 'meal_missing',
        ask: 'Which drivers had no meal break recorded in Cortex yesterday?',
        format: "full names separated by commas, or 'none'",
        grade: 'names',
        expected: [noMeal.join(', ') || 'none'],
      },
      {
        id: 'paycom_code',
        ask: "What is Taylor Brooks's Paycom employee code?",
        format: 'the code, such as E001',
        grade: 'text',
        expected: [code],
      },
    ];
    return { hoursWeek, questions };
  } finally {
    for (const db of [paycom, cortex]) db.close();
  }
}
