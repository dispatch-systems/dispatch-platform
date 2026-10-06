// Expectations read the source database independently of the tools agents call.
import { open, type Question, type World } from '../../../../core/mcp/tests/support/questions.js';
export function dailyPerformanceQuestions(world: World): Question[] {
  const db = open(world, 'daily_performance');
  try {
    const scope =
      ' FROM daily_rows x JOIN daily_publications p ON p.id=x.publication_id WHERE p.active=1 AND x.date BETWEEN ? AND ?';
    const count = (sql: string) =>
      String((db.prepare(sql).get(world.from, world.to) as { count: number }).count);
    const common = {
      format: 'End with one line: the number.',
      grade: 'number' as const,
      requiredToolEvidence: '"source":"daily_performance"',
    };
    return [
      {
        ...common,
        id: 'daily_safety_records',
        ask: `How many assessed daily safety event records were collected from ${world.from} to ${world.to}? Use Daily Performance and keep live events separate.`,
        expected: [count('SELECT count(*) count' + scope + " AND x.dataset='safety_events'")],
      },
      {
        ...common,
        id: 'daily_feedback_mishandled',
        ask: `How many daily mishandled-package feedback responses were collected from ${world.from} to ${world.to}? Use Daily Performance response counts.`,
        expected: [
          count(
            "SELECT sum(json_extract(x.row,'$.driver_mishandled_package_cnt')) count" +
              scope +
              " AND x.dataset='driver_feedback'",
          ),
        ],
      },
      {
        ...common,
        id: 'daily_business_closed',
        ask: `How many daily business-closed return records affected DCR from ${world.from} to ${world.to}? Use Daily Performance and its daily impact flag.`,
        expected: [
          count(
            'SELECT count(*) count' +
              scope +
              " AND x.dataset='returns_to_station' AND x.impact=1 AND json_extract(x.row,'$.rts_reason_code')='BUSINESS CLOSED'",
          ),
        ],
      },
    ];
  } finally {
    db.close();
  }
}
