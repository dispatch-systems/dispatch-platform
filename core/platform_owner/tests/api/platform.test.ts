import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { demo, fixture } from '../../../shell/tests/support/support.js';

// Core's own routes: what only the platform owner reads. Switching a DSP's features is tested
// with the features it switches, in app/tests/api/feature-switches.test.ts.
test('only the platform owner reads the platform: its DSPs, health and diagnostics', async (t) => {
  const f = await fixture();
  t.after(f.close);
  assert.equal((await f.request('/api/platform/health')).status, 401);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  assert.equal((await member.get('/api/platform/dsps')).status, 403);
  const diagnostics = await owner.get('/api/platform/diagnostics');
  assert(diagnostics.value.runtime.coreMemoryBytes > 0);
  assert.equal(diagnostics.value.runtime.workerMemoryBytes, 0);
});

test('a disabled DSP signs its people out and lets none in, until enabling it turns it back on as it was', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client(demo.member);
  const north = member.session.dsps[0];
  await member.select(north.id);
  const summary = async () =>
    (await owner.read('/api/platform/dsps')).find((dsp: { id: string }) => dsp.id === north.id);
  const before = await summary();
  assert.notDeepEqual(before.connections, {});
  assert.equal((await owner.post(`/api/platform/dsps/${north.id}/disable`)).status, 200);
  // The platform owner still has the admin's pages; its people have no session, nor a new one.
  assert.equal((await summary()).status, 'suspended');
  assert.equal((await member.get('/api/session')).status, 401);
  const login = { email: demo.member, password: demo.password };
  const refused = await f.request('/api/auth/login', login, f.at(demo.memberDsp));
  assert.deepEqual([refused.status, refused.value.error], [403, 'dsp_disabled']);
  assert.equal((await owner.post(`/api/platform/dsps/${north.id}/enable`)).status, 200);
  // Enabled, its features and connections are as they were; the sessions it ended stay ended.
  const after = await summary();
  assert.deepEqual([after.features, after.connections], [before.features, before.connections]);
  assert.equal((await member.get('/api/session')).status, 401);
  const back = await f.client(demo.member);
  await back.select(north.id);
});

test('deleting a disabled DSP leaves nothing of it, frees its code and unpairs its phones, and leaves another as it was', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsps = await owner.read('/api/platform/dsps');
  const named = (name: string) => dsps.find((dsp: { name: string }) => dsp.name === name);
  const [north, summit] = [named('Northline Logistics'), named('Summit Delivery')];
  const permanent = dsps.find((dsp: { permanent: boolean }) => dsp.permanent);
  const remove = async (dsp: { id: string }, name: string) => {
    const answer = await owner.post(`/api/platform/dsps/${dsp.id}/delete`, { name });
    return [answer.status, answer.value.error];
  };
  // What each DSP has across the platform: a key given both, a paired phone, and a row of each
  // kind the platform keeps for a DSP, a legacy account that is its alone among them.
  const key = await owner.post('/api/platform/agents/keys', {
    name: 'Both',
    allDsps: false,
    dsps: [north.id, summit.id],
    access: 'read',
    allTools: true,
    tools: {},
    expiresAt: null,
  });
  assert.equal(key.status, 200, key.body);
  await owner.select(north.id);
  const pairing = await owner.post('/api/dsp/communications/phones/pairing');
  const phone = { code: pairing.value.code, number: '', model: 'Pixel 8a', version: '1.4.0' };
  const paired = await f.request('/api/communications/phone/pair', phone, f.at(demo.memberDsp));
  assert.equal(paired.status, 200, paired.body);
  const platform = <T>(work: (db: import('node:sqlite').DatabaseSync) => T) =>
    f.database('data/platform/accounts.sqlite', work);
  for (const [n, dsp] of [north.id, summit.id].entries()) {
    platform((db) =>
      db.exec(`
        INSERT INTO slack_teams(team_id,dsp_id,connected_at) VALUES ('T${n}','${dsp}','');
        INSERT INTO driver_codes(code,dsp_id,created_at) VALUES ('C${n}','${dsp}','');
        INSERT INTO invitation_routes(hash,dsp_id) VALUES ('h${n}','${dsp}');
        INSERT INTO outbox(id,encrypted_message,status,available_at,dsp_id)
          VALUES ('o${n}','','sent',0,'${dsp}');
        INSERT INTO support_tickets(id,request_id,reporter_id,reporter_scope,reporter_name,dsp_id,
          kind,title,description,release,source_sha,created_at,updated_at)
          VALUES ('support_${n}','r','u','${dsp}','R','${dsp}','bug','t','d','r','s',0,0);
        INSERT INTO support_messages(id,ticket_id,author,role,body,private,created_at)
          VALUES ('m${n}','support_${n}','u','reporter','b',0,0);
        INSERT INTO agent_activity(at,key_id,key_name,key_kind,surface,dsp_id,outcome,ms,bytes)
          VALUES (0,'k','k','key','s','${dsp}','ok',0,0);
        INSERT INTO users(id,email,first_name,last_name,password,created_at)
          VALUES ('usr_${n}','legacy${n}@dispatch.test','L','P','x','');
        INSERT INTO memberships(id,user_id,dsp_id,role) VALUES ('mem_${n}','usr_${n}','${dsp}','member');
        INSERT INTO sessions(hash,user_id,user_version,expires_at,created_at)
          VALUES ('s${n}','usr_${n}',1,9000000000000,0);
      `),
    );
  }
  const held = (dsp: string) =>
    platform((db) => {
      const count = (sql: string) => Number(Object.values(db.prepare(sql).get(dsp)!)[0]);
      return [
        ...[
          'dsps',
          'dsp_features',
          'memberships',
          'invitation_routes',
          'audit',
          'agent_key_dsps',
          'slack_teams',
          'driver_codes',
          'outbox',
          'support_tickets',
          'agent_activity',
        ].map((table) =>
          count(`SELECT count(*) FROM ${table} WHERE ${table === 'dsps' ? 'id' : 'dsp_id'}=?`),
        ),
        count(`SELECT count(*) FROM support_messages WHERE ticket_id IN
          (SELECT id FROM support_tickets WHERE dsp_id=?)`),
        count(`SELECT count(*) FROM users u JOIN sessions s ON s.user_id=u.id
          WHERE u.email LIKE 'legacy%' AND u.id IN (SELECT user_id FROM memberships WHERE dsp_id=?)`),
      ];
    });
  assert(
    held(north.id).every((rows) => rows > 0),
    String(held(north.id)),
  );
  const summitHeld = held(summit.id);

  // Never an active DSP, the permanent one, or by a name not its own.
  assert.deepEqual(await remove(north, north.name), [409, 'dsp_not_disabled']);
  assert.deepEqual(await remove(permanent, permanent.name), [409, 'permanent_dev_required']);
  assert.equal((await owner.post(`/api/platform/dsps/${north.id}/disable`)).status, 200);
  assert.deepEqual(await remove(north, north.name.toUpperCase()), [400, 'dsp_name_mismatch']);
  assert(fs.existsSync(path.join(f.root, 'dsps', north.id)));

  assert.deepEqual(await remove(north, north.name), [200, undefined]);
  assert(!fs.existsSync(path.join(f.root, 'dsps', north.id)));
  assert.deepEqual(held(north.id), Array(13).fill(0));
  const [legacy, deletions, deleted] = platform((db) => [
    db.prepare("SELECT count(*) n FROM users WHERE id='usr_0'").get()!.n,
    db.prepare('SELECT count(*) n FROM dsp_deletions').get()!.n,
    db
      .prepare("SELECT detail,dsp_id FROM audit WHERE action='dsp.deleted'")
      .all()
      .map((row) => ({ ...row })),
  ]);
  assert.deepEqual([legacy, deletions], [0, 0]);
  assert.deepEqual(deleted, [{ detail: north.name, dsp_id: null }]);
  f.database('data/preview/jobs.sqlite', (db) => {
    for (const table of ['jobs', 'live_reads', 'live_days']) {
      const rows = db.prepare(`SELECT count(*) n FROM ${table} WHERE dsp_id=?`).get(north.id)!;
      assert.equal(rows.n, 0, table);
    }
  });
  // Its phone is told it's removed, and unpairs.
  const statuses = { statuses: [{ id: 'text_x', attempt: 1, status: 'sent', at: 1 }] };
  const told = await f.request('/api/communications/phone/statuses', statuses, {
    ...f.at(demo.memberDsp),
    authorization: `Bearer ${paired.value.key}`,
  });
  assert.deepEqual([told.status, told.value.error], [401, 'phones_removed']);
  // Summit keeps all it had, and may take the code that is free now.
  assert.deepEqual(held(summit.id), summitHeld);
  assert(fs.existsSync(path.join(f.root, 'dsps', summit.id)));
  const moved = await owner.post(`/api/platform/dsps/${summit.id}/code`, { code: demo.memberDsp });
  assert.equal(moved.status, 200, moved.body);
});
