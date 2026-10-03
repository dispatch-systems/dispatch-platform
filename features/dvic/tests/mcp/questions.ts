import { open, type Question, type World } from '../../../../core/mcp/tests/support/questions.js';

/** What agents are asked about DVIC inspections, and who was short, whom other questions ask
 * about too. */
export function dvicQuestions(world: World) {
  const dvic = open(world, 'dvic');
  try {
    const fortnight = `from ${world.from} to ${world.to}`;
    const short = (
      dvic
        .prepare(
          `SELECT DISTINCT transporter_name name FROM dvic_inspections
           WHERE short=1 AND start_date BETWEEN ? AND ? ORDER BY name`,
        )
        .all(world.from, world.to) as { name: string }[]
    ).map((row) => row.name);

    const questions: Question[] = [
      {
        id: 'short_dvic_default',
        ask: 'Which drivers were short on their DVIC?',
        format: "full names separated by commas, or 'none'",
        grade: 'names',
        expected: [short.join(', ') || 'none'],
      },
      {
        id: 'short_dvic_drivers',
        ask: `Which drivers had at least one short DVIC inspection ${fortnight}?`,
        format: "full names separated by commas, or 'none'",
        grade: 'names',
        expected: [short.join(', ') || 'none'],
      },
    ];
    return { short, questions };
  } finally {
    dvic.close();
  }
}

/** How many short inspections a driver had in the fortnight. */
export function shortInspections(world: World, name: string) {
  const dvic = open(world, 'dvic');
  try {
    return (
      dvic
        .prepare(
          `SELECT COUNT(*) n FROM dvic_inspections WHERE short=1 AND transporter_name=?
             AND start_date BETWEEN ? AND ?`,
        )
        .get(name, world.from, world.to) as { n: number }
    ).n;
  } finally {
    dvic.close();
  }
}
