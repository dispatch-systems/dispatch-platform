import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fixture, prepare, until } from '../../../shell/tests/support/support.js';
import worker from '../../../../services/cloudflare-mail/worker.js';
import type { Env } from '../../../../services/cloudflare-mail/worker-configuration.js';

const token = 'synthetic-worker-secret-for-invite-tests';

test('Cloudflare outbox sends HTML, retries failure and clears encrypted mail after acceptance', async (t) => {
  const received: { headers: http.IncomingHttpHeaders; message: any }[] = [];
  let rejectMail = true;
  const server = http.createServer(async (req, res) => {
    let body = '';
    for await (const chunk of req) body += chunk;
    received.push({ headers: req.headers, message: JSON.parse(body) });
    res.writeHead(rejectMail ? 502 : 200, { 'content-type': 'application/json' });
    res.end(JSON.stringify(rejectMail ? { error: 'email_delivery_failed' } : { ok: true }));
  });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise<void>((resolve) => server.close(() => resolve())));
  const endpoint = `http://127.0.0.1:${(server.address() as { port: number }).port}/send`;
  const f = await fixture({
    seed: false,
    env: {
      DISPATCH_DEV_MAIL_MODE: 'cloudflare',
      DISPATCH_DEV_MAIL_WORKER_URL: endpoint,
      DISPATCH_DEV_MAIL_WORKER_TOKEN: token,
    },
  });
  t.after(f.close);
  const owner = await f.client();
  assert.equal(
    (await owner.post('/api/platform/dsps', { ownerEmail: 'delivery@dispatch.test' })).status,
    201,
  );
  const row = () =>
    f.database('data/platform/accounts.sqlite', (db) => db.prepare('SELECT * FROM outbox').get())!;
  await until(async () => row().attempts === 1);
  assert.equal(row().status, 'pending');
  const waiting = (await owner.get('/api/platform/health')).value.mail;
  assert.equal(waiting.pending, 1);
  assert.equal(waiting.failed, 0);
  assert.equal(waiting.lastError, 'email_http_502');
  assert(waiting.oldestPendingAgeMs >= 0);
  assert(waiting.lastAttemptAt);
  assert.equal(waiting.lastSuccessAt, null);
  assert(row().encrypted_message);
  assert.equal(received[0]!.headers.authorization, `Bearer ${token}`);
  assert.equal(received[0]!.message.origin, f.env.DISPATCH_ORIGIN);
  assert.match(received[0]!.message.html, /Start DSP onboarding/);
  assert.equal(fs.existsSync(path.join(f.root, 'data/platform/development-mail')), false);
  rejectMail = false;
  f.database('data/platform/accounts.sqlite', (db) =>
    db.prepare('UPDATE outbox SET available_at=0').run(),
  );
  await until(async () => row().status === 'sent');
  assert.equal(row().encrypted_message, '');
  const delivered = (await owner.get('/api/platform/health')).value.mail;
  assert.equal(delivered.pending, 0);
  assert.equal(delivered.lastError, null);
  assert(delivered.lastSuccessAt);
  assert.equal(delivered.oldestPendingAgeMs, null);
  assert.equal((await f.request('/api/platform/health')).status, 401);
  assert.equal(f.logs().includes(token), false);
  assert.equal(f.logs().includes('delivery@dispatch.test'), false);
  // A terminal failure stays visible after restarting the core.
  rejectMail = true;
  await owner.post('/api/platform/dsps', { ownerEmail: 'failed@dispatch.test' });
  // Queued mail goes out at once: exhaust its attempts only after the first one is recorded,
  // so the edit cannot race a delivery in flight.
  const pending = () =>
    f.database('data/platform/accounts.sqlite', (db) =>
      db.prepare("SELECT attempts FROM outbox WHERE status='pending'").get(),
    ) as { attempts: number } | undefined;
  await until(async () => pending()?.attempts === 1);
  f.database('data/platform/accounts.sqlite', (db) =>
    db.prepare("UPDATE outbox SET attempts=4,available_at=0 WHERE status='pending'").run(),
  );
  await until(async () => (await owner.read('/api/platform/health')).mail.failed === 1);
  await f.stop();
  await f.start();
  const failure = (await owner.get('/api/platform/health')).value.mail;
  assert.equal(failure.failed, 1);
  assert.equal(failure.lastError, 'email_http_502');
  assert(failure.lastSuccessAt);
});

test('unsafe mail configuration is rejected without disclosing it', async (t) => {
  const f = await prepare(false);
  t.after(f.cleanup);
  const privateValue = 'smtp://private-password@relay.example';
  const result = spawnSync(f.binary, ['status'], {
    env: {
      ...f.env,
      DISPATCH_DEV_MAIL_MODE: 'smtp',
      DISPATCH_DEV_SMTP_URL: privateValue,
      DISPATCH_DEV_MAIL_FROM: 'sender@example.test',
    },
    encoding: 'utf8',
  });
  const output = `${result.stdout}${result.stderr}`;
  assert.notEqual(result.status, 0);
  assert(output.includes('mail_configuration_required'));
  assert(!output.includes(privateValue));
});

test('Cloudflare Worker requires the private secret and matching environment before sending', async () => {
  const sent: unknown[] = [];
  const env: Env = {
    DISPATCH_ENVIRONMENT: 'preview',
    DISPATCH_ORIGIN: 'https://dev.dispatch.test',
    MAIL_FROM: 'no-reply@dev.dispatch.test',
    MAIL_TOKEN: token,
    EMAIL: {
      send: async (message) => {
        sent.push(message);
        return { messageId: 'synthetic-id' };
      },
    },
  };
  const message = {
    to: 'recipient@example.com',
    subject: '[Dispatch Dev] Set up your DSP',
    text: 'Open Dev onboarding',
    html: '<a>Start DSP onboarding</a>',
    environment: 'preview',
    origin: env.DISPATCH_ORIGIN,
  };
  const send = (body: unknown = message, secret = token) =>
    worker.fetch(
      new Request('https://mail.example/send', {
        method: 'POST',
        headers: { authorization: `Bearer ${secret}`, 'content-type': 'application/json' },
        body: JSON.stringify(body),
      }),
      env,
    );
  assert.equal((await send(message, 'wrong')).status, 401);
  assert.equal((await send({ ...message, environment: 'production' })).status, 403);
  assert.equal((await send({ ...message, origin: 'https://dispatch.test' })).status, 403);
  assert.equal(
    (await send({ ...message, to: ['one@example.com', 'two@example.com'] })).status,
    400,
  );
  assert.equal((await send({ ...message, text: 'a'.repeat(20000) })).status, 413);
  assert.equal(sent.length, 0);
  assert.equal((await send({ ...message, from: 'spoof@example.com' })).status, 200);
  assert.deepEqual(sent, [
    {
      from: { name: 'Dispatch', email: env.MAIL_FROM },
      to: message.to,
      subject: message.subject,
      text: message.text,
      html: message.html,
    },
  ]);
  env.EMAIL.send = async () => {
    throw new Error('private provider details');
  };
  const failure = await send();
  assert.equal(failure.status, 502);
  assert.deepEqual(await failure.json(), { error: 'email_delivery_failed' });
});
