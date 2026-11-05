import { createHash } from 'node:crypto';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import test, { type TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { demo, fixture, until } from '../../../../core/shell/tests/support/support.js';

// A phone pairs at its DSP's own address with a code someone who manages the DSP's connections
// made, and then speaks only with the key it was given, there and nowhere else. The app it runs
// is anyone's to download there.

type Fixture = Awaited<ReturnType<typeof fixture>>;

test('A phone pairs once with a code, then calls with its key at its DSP alone', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  const before = await owner.get('/api/dsp/communications/phones');
  assert.equal(before.status, 200, before.body);

  const pairing = await owner.post('/api/dsp/communications/phones/pairing');
  assert.equal(pairing.status, 200, pairing.body);
  assert.match(pairing.value.code, /^[A-HJ-NP-Z2-9]{8}$/);
  assert.match(pairing.value.picture, /^data:image\/svg\+xml;base64,/);

  const at = f.at(demo.memberDsp);
  const phone = {
    code: pairing.value.code,
    number: '(206) 555-0177',
    model: 'Pixel 8a',
    version: '1.0.0',
  };
  // Only at a DSP's own address, and never from a browser.
  assert.equal((await f.request('/api/communications/phone/pair', phone)).status, 401);
  const browser = await f.request('/api/communications/phone/pair', phone, {
    ...at,
    cookie: 'dispatch_session=x',
  });
  assert.equal(browser.status, 401);
  const paired = await f.request('/api/communications/phone/pair', phone, at);
  assert.equal(paired.status, 200, paired.body);
  assert.match(paired.value.key, /^[0-9a-f]{64}$/);
  assert.equal(paired.value.dsp.name, 'Northline Logistics');
  const again = await f.request('/api/communications/phone/pair', phone, at);
  assert.equal(again.value.error, 'phones_pairing_invalid');

  const listed = await owner.get('/api/dsp/communications/phones');
  const row = listed.value.phones.find((p: { id: string }) => p.id === paired.value.phone.id);
  assert.equal(row.number, '+12065550177');
  assert.equal(row.online, true);

  const renamed = await owner.post(
    `/api/dsp/communications/phones/${paired.value.phone.id}/rename`,
    {
      name: 'Station 2',
    },
  );
  assert.equal(renamed.status, 200, renamed.body);
  assert.equal(renamed.value.name, 'Station 2');

  const key = { authorization: `Bearer ${paired.value.key}` };
  const text = {
    clientId: 'sms-1',
    direction: 'in',
    number: '4255550199',
    body: 'Gate code?',
    at: Date.now(),
  };
  const reported = await f.request(
    '/api/communications/phone/texts',
    { texts: [text] },
    { ...at, ...key },
  );
  assert.equal(reported.status, 200, reported.body);
  assert.equal(reported.value.texts[0].clientId, 'sms-1');
  // The key works at its DSP's address alone, and only while the phone is paired.
  const elsewhere = await f.request('/api/communications/phone/texts', { texts: [text] }, key);
  assert.equal(elsewhere.status, 401);
  const wrong = { authorization: `Bearer ${'0'.repeat(64)}` };
  assert.equal(
    (await f.request('/api/communications/phone/texts', { texts: [text] }, { ...at, ...wrong }))
      .status,
    401,
  );

  // While Communications is off, managing all the DSP's connections still lets the owner in, but
  // its phones aren't there to manage, and the phone's key reaches nothing.
  const switches = `/api/platform/dsps/${north.id}/features`;
  const off = { feature: 'communications', enabled: false };
  assert.equal((await owner.post(switches, off)).status, 200);
  await owner.select(north.id);
  assert.equal((await owner.get('/api/dsp/communications/phones')).status, 404);
  assert.equal((await owner.post('/api/dsp/communications/phones/pairing')).status, 404);
  const asleep = await f.request(
    '/api/communications/phone/texts',
    { texts: [text] },
    { ...at, ...key },
  );
  assert.equal(asleep.value.error, 'phones_off');
  assert.equal((await owner.post(switches, { ...off, enabled: true })).status, 200);
  await owner.select(north.id);

  const removed = await owner.post(
    `/api/dsp/communications/phones/${paired.value.phone.id}/remove`,
  );
  assert.equal(removed.status, 200, removed.body);
  const after = await f.request(
    '/api/communications/phone/statuses',
    { statuses: [{ id: 'text_x', attempt: 1, status: 'sent', at: 1 }] },
    { ...at, ...key },
  );
  assert.equal(after.status, 401);
  assert.equal(after.value.error, 'phones_removed');
});

test("Only those who manage the DSP's phones or connections see or pair them", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const member = await f.client(demo.member);
  const north = member.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await member.select(north.id);
  assert.equal((await member.get('/api/dsp/communications/phones')).status, 403);
  assert.equal((await member.post('/api/dsp/communications/phones/pairing')).status, 403);
});

test('A phone says how it stands with each wait, which its card and every page show', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  const at = f.at(demo.memberDsp);
  const pairing = await owner.post('/api/dsp/communications/phones/pairing');
  const paired = await f.request(
    '/api/communications/phone/pair',
    { code: pairing.value.code, number: '', model: 'Pixel 8a', version: '1.4.0' },
    at,
  );
  const id = paired.value.phone.id;
  const key = { ...at, authorization: `Bearer ${paired.value.key}` };
  // A text waiting for it to send, so each wait answers at once.
  const text = { clientId: 'sms-1', direction: 'in', number: '4255550142', body: 'Hi', at: 1 };
  await f.request('/api/communications/phone/texts', { texts: [text] }, key);
  let conversation = '';
  await until(async () => {
    const listed: { id: string; number?: string }[] = (await owner.read('/api/dsp/communications'))
      .conversations;
    conversation = listed.find((each) => each.number === '+14255550142')?.id ?? '';
    return conversation !== '';
  });
  await owner.post(`/api/dsp/communications/conversations/${conversation}/messages`, {
    text: 'Hello',
  });
  const wait = (health?: unknown) =>
    f.request(
      '/api/communications/phone/outbox',
      undefined,
      health === undefined
        ? key
        : {
            ...key,
            'x-dispatch-health': typeof health === 'string' ? health : JSON.stringify(health),
          },
    );
  const card = async () =>
    (await owner.read('/api/dsp/communications/phones')).phones.find(
      (phone: { id: string }) => phone.id === id,
    ).health;
  /** The phones every page's banner reads, the demo's stand-in left out. */
  const banner = async () =>
    (await owner.read('/api/dsp/communications/waiting')).phones.map(
      (phone: { id: string; cantSend: boolean }) => [phone.id, phone.cantSend],
    );

  // An app older than that says nothing of it.
  assert.equal((await wait()).status, 200);
  assert.equal(await card(), null);
  assert.deepEqual(await banner(), [[id, false]]);

  const health = {
    setup: ['battery', 'chat_features'],
    read: true,
    send: false,
    sim: 'ready',
    waiting: { texts: 12, files: 1, statuses: 0 },
    undownloaded: 2,
    sendError: { error: "Sending texts isn't allowed on the phone", at: 1 },
    unexpected: { error: 'java.lang.IllegalStateException', where: 'Reading texts', at: 2 },
  };
  assert.equal((await wait(health)).status, 200);
  assert.deepEqual(await card(), {
    cantSend: true,
    cantRead: false,
    noSim: false,
    noSignal: false,
    setupUndone: true,
    waiting: 12,
    undownloaded: 2,
  });
  assert.deepEqual(await banner(), [[id, true]]);
  // With no SIM it can't send either; with one and no signal, it can once there is.
  const absent = { ...health, send: true, sim: 'absent' };
  assert.equal((await wait(absent)).status, 200);
  assert.equal((await card()).noSim, true);
  assert.deepEqual(await banner(), [[id, true]]);
  const well = { ...absent, setup: [], sim: 'no_signal', sendError: null, unexpected: null };
  assert.equal((await wait(well)).status, 200);
  assert.deepEqual(await banner(), [[id, false]]);

  // Past its limits it's refused, and what it said before kept: never a text or a number where
  // the app says what went wrong.
  for (const refused of [
    '{',
    { ...well, more: true },
    { ...well, setup: ['Battery'] },
    { ...well, waiting: { texts: -1, files: 0, statuses: 0 } },
    { ...well, sim: 'roaming' },
    { ...well, unexpected: { error: 'Call 2065550100', where: 'Reading texts', at: 1 } },
    { ...well, unexpected: { error: 'java.io.IOException', where: 'Texting 555 0100', at: 1 } },
    { ...well, sendError: { error: 'x'.repeat(501), at: 1 } },
    { ...well, setup: Array.from({ length: 400 }, () => 'battery') },
  ]) {
    const answer = await wait(refused);
    assert.equal(answer.value.error, 'invalid_input', JSON.stringify(refused).slice(0, 80));
  }
  assert.equal((await card()).noSignal, true);
});

/** Each build: its version and versionCode, and its APK, as a release's package carries them. */
const BUILDS = {
  dev: { version: '1.3.1-dev.14', versionCode: 14, apk: 'dev apk' },
  production: { version: '1.3.0', versionCode: 12, apk: 'production apk' },
};
const sha256 = (text: string) => createHash('sha256').update(text).digest('hex');

/** A runtime with the app packed in `dashboard/relay/`, as `dispatchdev build` packs it. */
function packed(t: TestContext) {
  const runtime = fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), 'dispatch-relay-'));
  t.after(() => fs.rmSync(runtime, { recursive: true, force: true }));
  const relay = path.join(runtime, 'dashboard/relay');
  fs.mkdirSync(relay, { recursive: true });
  const builds = Object.fromEntries(
    Object.entries(BUILDS).map(([build, { version, versionCode, apk }]) => {
      const asset = `dsp-dispatch-relay-${version}.apk`;
      fs.writeFileSync(path.join(relay, asset), apk);
      const commit = '1'.repeat(40);
      return [
        build,
        { version, versionCode, commit, asset, sha256: sha256(apk), size: apk.length },
      ];
    }),
  );
  fs.writeFileSync(path.join(relay, 'relay.json'), JSON.stringify(builds));
  return runtime;
}
/** A GET at the server, its body as bytes. */
const fetched = (f: Fixture, url: string, headers: Record<string, string>) =>
  new Promise<{ status: number; headers: http.IncomingHttpHeaders; body: Buffer }>(
    (resolve, reject) => {
      http
        .get(`http://127.0.0.1:${f.env.PORT}${url}`, { headers }, (response) => {
          const chunks: Buffer[] = [];
          response.on('data', (chunk: Buffer) => chunks.push(chunk));
          response.on('end', () =>
            resolve({
              status: response.statusCode!,
              headers: response.headers,
              body: Buffer.concat(chunks),
            }),
          );
        })
        .once('error', reject);
    },
  );

test("A DSP's address hands out its server's build of the app to anyone, and phones hear of each version", async (t) => {
  const f = await fixture({ env: { DISPATCH_ARTIFACT_ROOT: packed(t) } });
  t.after(f.close);
  const owner = await f.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  const at = f.at(demo.memberDsp);

  // The card links to its download at the DSP's own address, with that as a QR code.
  const { app } = await owner.read('/api/dsp/communications/phones');
  assert.deepEqual(
    [app.name, app.version, app.link],
    [
      'DSP Dispatch: Relay Dev',
      '1.3.1-dev.14',
      `http://${at.host}/api/communications/phone/app/download`,
    ],
  );
  assert.match(app.picture, /^data:image\/svg\+xml;base64,/);

  // Anyone at the address reads it, a browser too, as a phone installs it before it's paired: a
  // preview server's is Dev's build.
  for (const headers of [at, { ...at, cookie: 'dispatch_session=x' }]) {
    const details = await f.request('/api/communications/phone/app', undefined, headers);
    assert.equal(details.status, 200, details.body);
    assert.deepEqual(details.value, {
      version: '1.3.1-dev.14',
      versionCode: 14,
      size: BUILDS.dev.apk.length,
      sha256: sha256(BUILDS.dev.apk),
    });
    const download = await fetched(f, '/api/communications/phone/app/download', headers);
    assert.equal(download.status, 200);
    assert.equal(download.headers['content-type'], 'application/vnd.android.package-archive');
    assert.match(
      download.headers['content-disposition']!,
      /dsp-dispatch-relay-1\.3\.1-dev\.14\.apk/,
    );
    assert.equal(download.body.toString(), BUILDS.dev.apk);
  }
  assert.equal((await f.request('/api/communications/phone/app')).status, 401);

  // A phone hears of the latest version as its outbox answers.
  const pairing = await owner.post('/api/dsp/communications/phones/pairing');
  const paired = await f.request(
    '/api/communications/phone/pair',
    { code: pairing.value.code, number: '', model: 'Pixel 8a', version: '1.3.0' },
    at,
  );
  const key = { ...at, authorization: `Bearer ${paired.value.key}` };
  const text = { clientId: 'sms-1', direction: 'in', number: '4255550142', body: 'Hi', at: 1 };
  assert.equal(
    (await f.request('/api/communications/phone/texts', { texts: [text] }, key)).status,
    200,
  );
  let conversation = '';
  await until(async () => {
    const listed: { id: string; number?: string }[] = (await owner.read('/api/dsp/communications'))
      .conversations;
    conversation = listed.find((each) => each.number === '+14255550142')?.id ?? '';
    return conversation !== '';
  });
  const answered = await owner.post(
    `/api/dsp/communications/conversations/${conversation}/messages`,
    {
      text: 'Hello',
    },
  );
  assert.equal(answered.status, 201, answered.body);
  const outbox = await f.request('/api/communications/phone/outbox', undefined, key);
  assert.equal(outbox.value.texts[0].body, 'Hello');
  assert.deepEqual(outbox.value.app, { version: '1.3.1-dev.14', versionCode: 14 });

  // It says the app it runs with each wait, kept as its own: one older than the version handed
  // out has an update. Paired, it said no versionCode.
  const listed = async () => {
    const { phones } = await owner.read('/api/dsp/communications/phones');
    const row = phones.find((p: { id: string }) => p.id === paired.value.phone.id);
    return [row.appVersion, row.update];
  };
  assert.deepEqual(await listed(), ['1.3.0', false]);
  for (const [version, versionCode, update] of [
    ['1.3.0', 12, true],
    ['1.3.1-dev.14', 14, false],
  ] as const) {
    const query = `version=${version}&versionCode=${versionCode}`;
    const waited = await f.request(`/api/communications/phone/outbox?${query}`, undefined, key);
    assert.equal(waited.status, 200, waited.body);
    assert.deepEqual(await listed(), [version, update]);
  }
  for (const query of ['version=1.4.1', 'versionCode=15', 'version=1.4.1&versionCode=0']) {
    const refused = await f.request(`/api/communications/phone/outbox?${query}`, undefined, key);
    assert.equal(refused.value.error, 'invalid_input', query);
  }
  assert.deepEqual(await listed(), ['1.3.1-dev.14', false]);

  // While Communications is off for the DSP, its address hands out nothing.
  const switches = `/api/platform/dsps/${north.id}/features`;
  assert.equal(
    (await owner.post(switches, { feature: 'communications', enabled: false })).status,
    200,
  );
  const off = await f.request('/api/communications/phone/app', undefined, at);
  assert.equal(off.value.error, 'phones_off');
});
