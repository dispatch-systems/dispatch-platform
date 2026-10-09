import assert from 'node:assert/strict';
import { fixture, until } from '../../../../core/shell/tests/support/support.js';

/** Northline Logistics with its station and a Cortex connection, ready to collect DVIC. */
export async function connectDvic(app: Awaited<ReturnType<typeof fixture>>) {
  const owner = await app.client();
  const dsp = owner.session.dsps.find(
    (item: { name: string }) => item.name === 'Northline Logistics',
  );
  await owner.select(dsp.id);
  assert.equal(
    (
      await owner.post('/api/dsp/profile', {
        name: dsp.name,
        abbreviation: 'NLL',
        stationCode: 'TST1',
        timezone: dsp.timezone,
      })
    ).status,
    200,
  );
  await owner.select(dsp.id);
  assert.equal(
    (
      await owner.post('/api/dsp/connections/cortex', {
        username: 'fixture@example.test',
        password: 'fixture-password',
      })
    ).status,
    200,
  );
  return { owner, dsp };
}

/** Synthetic inspections in the real DVIC store, for the daily log's browser checks. */
export async function seedDvic(app: Awaited<ReturnType<typeof fixture>>, count = 19) {
  const { owner, dsp } = await connectDvic(app);
  const job = await owner.post('/api/dsp/dvic/collect', {
    requestId: 'daily-log-fixture',
    week: '2026-W39',
  });
  assert.equal(job.status, 202, job.body);
  await until(async () => {
    const status = await owner.read('/api/dsp/dvic/status');
    const current = status.jobs.find((item: { id: string }) => item.id === job.value.id);
    assert.notEqual(current?.status, 'failed', JSON.stringify(current));
    return current?.status === 'succeeded';
  });
  app.database(`dsps/${dsp.id}/data/dvic/dvic.sqlite`, (db) => {
    const source = db.prepare('SELECT revision_id FROM dvic_inspections LIMIT 1').get()!;
    const names = [
      'Taylor Brooks',
      'Jordan Lee',
      'Morgan Ellis',
      'Alex Rivera',
      'Casey Patel',
      'Riley Chen',
      'Sam Parker',
      'Avery Scott',
    ];
    db.exec('BEGIN');
    db.exec('DELETE FROM dvic_inspections');
    const insert = db.prepare(
      `INSERT INTO dvic_inspections(company_id,inspection_key,dsp_code,station,start_date,
       transporter_id,transporter_name,vin,fleet_type,inspection_type,inspection_status,start_time,
       end_time,duration_seconds,minimum_seconds,short,report_date,source_modified_at,revision_id,
       scope_verified) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,1)`,
    );
    for (let i = 0; i < count; i++) {
      const date = '2026-09-' + (26 - (i % 7));
      const fleet = ['CV', 'SV', 'CDV'][i % 3]!;
      const seconds = [34, 142, 77, 58, 238, 63][i % 6]!;
      const start =
        date +
        'T09:' +
        String(Math.floor(i / 60) % 60).padStart(2, '0') +
        ':' +
        String(i % 60).padStart(2, '0');
      const end = new Date(new Date(start + 'Z').getTime() + seconds * 1000)
        .toISOString()
        .slice(0, 19);
      insert.run(
        'company-fixture',
        'page-row-' + i,
        'FXTR',
        'TST1',
        date,
        'DA-' + (1040 + (i % 8)),
        names[i % 8]!,
        '1FIXTURE' + String(i % 8).padStart(9, '0'),
        fleet,
        'PRE_TRIP_DVIC',
        'PASSED',
        start,
        end,
        seconds,
        fleet === 'SV' ? 300 : 90,
        1,
        '2026-09-27',
        1,
        source.revision_id!,
      );
    }
    db.prepare('UPDATE dvic_reports SET min_date=?,max_date=?').run('2026-09-20', '2026-09-26');
    db.exec('COMMIT');
  });
  return { owner, dsp };
}
