import { fixture, until } from './support.js';

type App = Awaited<ReturnType<typeof fixture>>;

/**
 * Northline's people as Paycom and Amazon write them differently: two preferred names, a
 * driver Amazon lists by a last initial, one Paycom has never heard of and one who drove
 * only long ago. The server restarts around the edit, so its first Driver Match pass
 * reads it.
 */
export async function seedDriverMatch(app: App) {
  const owner = await app.client();
  const dsp = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  const date = app.collector(dsp.id, (db) => {
    for (const [code, name] of [
      ['E009', 'COLLINS, BENJAMIN'],
      ['E011', 'MITCHELL, SAMANTHA'],
    ])
      db.prepare('UPDATE employees SET name=? WHERE code=?').run(name!, code!);
    return (db.prepare('SELECT max(date) date FROM timecards').get() as { date: string }).date;
  });
  await app.stop();
  const drivers = [
    ['match-avery', 'Avery Morgan'],
    ['match-jordan', 'Jordan Ellis'],
    ['match-taylor', 'Taylor Brooks'],
    ['match-ben', 'Ben Collins'],
    ['match-sam', 'Sam Mitchell'],
    ['match-drew', 'Drew S.'],
    ['match-dmitri', 'Dmitri Volkov'],
  ];
  app.database(`dsps/${dsp.id}/data/cortex/cortex.sqlite`, (db) => {
    db.prepare(
      `INSERT INTO meal_publications VALUES
       ('match-pub','match-job',?,'DEMO1','area-match','ALL_DRIVERS','UTC',?,?,1,?,0,0,3)`,
    ).run(date, `${date}T23:00:00Z`, `${date}T23:01:00Z`, drivers.length);
    for (const [id, name] of drivers)
      db.prepare(
        `INSERT INTO meal_itineraries VALUES
         ('match-pub',?,?,?,'R1',?,1,'complete','none_recorded')`,
      ).run(`itinerary-${id}`, id!, name!, `${date}T23:00:00Z`);
    const old = new Date(Date.parse(`${date}T00:00:00Z`) - 40 * 86400000)
      .toISOString()
      .slice(0, 10);
    db.prepare(
      `INSERT INTO meal_publications VALUES
       ('match-old','match-old-job',?,'DEMO1','area-match','ALL_DRIVERS','UTC',?,?,1,1,0,0,3)`,
    ).run(old, `${old}T23:00:00Z`, `${old}T23:01:00Z`);
    db.prepare(
      `INSERT INTO meal_itineraries VALUES
       ('match-old','itinerary-match-gone','match-gone','Riley Quinn','R1',?,1,'complete','none_recorded')`,
    ).run(`${old}T23:00:00Z`);
  });
  await app.start();
  await owner.select(dsp.id);
  await until(async () => (await owner.get('/api/dsp/driver-match')).value.review?.length >= 3);
  return dsp as { id: string; name: string };
}
