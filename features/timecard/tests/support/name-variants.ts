import type { fixture } from '../../../../core/shell/tests/support/support.js';

type App = Awaited<ReturnType<typeof fixture>>;
type Client = Awaited<ReturnType<App['client']>>;

/** Paycom's name for four of the demo's employees, and Amazon's for the same drivers. */
export const variants = [
  ['REED, JAMIE', 'Jamie Reed Vega'],
  ['STONE, ALEX', 'Alexander Stone'],
  ['MOLINA, CASEY', 'Casey Molina Solis'],
  ['HART, TAYLOR', 'Taylor Hart Jr'],
] as const;

/**
 * Four of the demo's employees under Paycom's names, each with meals on the newest day under
 * Amazon's: the first with two, the others with one. Answers that day's meal-break page as it
 * was before the meals.
 */
export async function seedNameVariants(app: App, owner: Client, dspId: string) {
  const date = app.collector(dspId, (db) => {
    const date = (db.prepare('SELECT max(date) date FROM timecards').get() as { date: string })
      .date;
    variants.forEach(([paycom], i) => {
      db.prepare('UPDATE employees SET name=? WHERE code=?').run(paycom, `E00${i + 1}`);
    });
    return date;
  });
  const endpoint = `/api/dsp/paycom/meal-breaks?date=${date}`;
  const before = await owner.read(endpoint);
  // Fixture writes bypass the collector's revision notification; restart after seeding.
  await app.stop();
  app.database(`dsps/${dspId}/data/cortex/cortex.sqlite`, (db) => {
    db.prepare(
      `INSERT INTO meal_publications VALUES
       ('variant-pub','variant-job',?,'DEMO1','area-1','ALL_DRIVERS','UTC',?,?,1,4,5,5,3)`,
    ).run(date, `${date}T23:00:00Z`, `${date}T23:01:00Z`);
    variants.forEach(([, flex], i) => {
      db.prepare(
        `INSERT INTO meal_itineraries VALUES
         ('variant-pub',?,?,?,'R1',?,1,'complete','recorded')`,
      ).run(`route-${i}`, `driver-${i}`, flex, `${date}T23:00:00Z`);
      for (const meal of i === 0 ? [0, 1] : [0]) {
        const hour = 12 + meal * 4;
        db.prepare(
          `INSERT INTO meal_records VALUES ('variant-pub',?,?,?,?,?,?,'verified','verified')`,
        ).run(
          `route-${i}`,
          `meal-${meal}`,
          `${date}T${hour - 1}:58:00Z`,
          `${date}T${hour}:00:00Z`,
          `${date}T${hour}:30:00Z`,
          `${date}T${hour}:32:00Z`,
        );
      }
    });
  });
  await app.start();
  return { date, endpoint, before };
}
