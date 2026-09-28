import test from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import type { AddressInfo } from 'node:net';
import { fixture } from '../support/support.js';

test('only an authenticated platform owner receives a short-lived playground ticket', async (t) => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-playground-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const key = 'b'.repeat(64);
  const keyFile = path.join(directory, 'key');
  fs.writeFileSync(keyFile, key, { mode: 0o600 });
  const f = await fixture({
    env: {
      DISPATCH_PLAYGROUND_ORIGIN: 'https://playground.example.com',
      DISPATCH_PLAYGROUND_KEY_FILE: keyFile,
    },
  });
  t.after(f.close);
  const route = '/api/platform/design-playground';
  assert.equal((await f.request(route)).status, 401);
  const member = await f.client('member@dispatch.test');
  assert.equal((await member.get(route)).status, 403);
  const owner = await f.client();
  const first = await owner.get(route);
  assert.equal(first.status, 200);
  assert.equal(first.value.origin, 'https://playground.example.com');
  const [encoded, signature] = first.value.ticket.split('.');
  assert.equal(signature, createHmac('sha256', key).update(encoded).digest('base64url'));
  const claims = JSON.parse(Buffer.from(encoded, 'base64url').toString());
  assert.equal(claims.aud, first.value.origin);
  assert.equal(claims.iss, f.env.DISPATCH_ORIGIN);
  assert.equal(claims.exp, first.value.expiresAt);
  assert(claims.exp > Date.now() && claims.exp <= Date.now() + 30000);
  assert(!JSON.stringify(claims).includes(owner.session.user.email));
  const second = await owner.get(route);
  assert.notEqual(first.value.ticket, second.value.ticket);
});

test('unconfigured playground returns no access material', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  assert.deepEqual((await owner.get('/api/platform/design-playground')).value, {
    origin: null,
    ticket: null,
    expiresAt: null,
  });
  assert.deepEqual((await owner.get('/api/platform/design-playground/status')).value, {
    configured: false,
    running: false,
    canStart: false,
  });
  assert.equal((await owner.post('/api/platform/design-playground/start', {})).status, 409);
});

test('playground health and start are owner-only, CSRF protected, and cannot select a service', async (t) => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-playground-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const keyFile = path.join(directory, 'key');
  fs.writeFileSync(keyFile, 'c'.repeat(64), { mode: 0o600 });
  let healthy = true;
  const service = http.createServer((req, res) => {
    assert.equal(req.url, '/health');
    res.setHeader('content-type', 'application/json');
    res.end(JSON.stringify({ ok: healthy }));
  });
  await new Promise<void>((resolve) => service.listen(0, '127.0.0.1', resolve));
  t.after(() => {
    service.closeAllConnections();
    service.close();
  });
  const port = (service.address() as AddressInfo).port;
  const f = await fixture({
    env: {
      DISPATCH_PLAYGROUND_ORIGIN: 'https://playground.example.com',
      DISPATCH_PLAYGROUND_KEY_FILE: keyFile,
      DISPATCH_PLAYGROUND_LOCAL_PORT: String(port),
      // Failure tests must never contact the host's real user service manager.
      DBUS_SESSION_BUS_ADDRESS: `unix:path=${directory}/missing-bus`,
      XDG_RUNTIME_DIR: directory,
    },
  });
  t.after(f.close);
  const status = '/api/platform/design-playground/status';
  const start = '/api/platform/design-playground/start';
  assert.equal((await f.request(status)).status, 401);
  assert.equal((await f.request(start, {})).status, 401);
  const member = await f.client('member@dispatch.test');
  assert.equal((await member.get(status)).status, 403);
  assert.equal((await member.post(start, {})).status, 403);
  const owner = await f.client();
  assert.equal((await f.request(start, {}, { cookie: owner.headers.cookie! })).status, 403);
  assert.equal((await owner.post(start, { service: 'other.service' })).status, 400);
  const running = { configured: true, running: true, canStart: true };
  assert.deepEqual((await owner.get(status)).value, running);
  // Starting an already healthy service succeeds without invoking systemctl.
  assert.deepEqual((await owner.post(start, {})).value, running);
  healthy = false;
  assert.deepEqual((await owner.get(status)).value, { ...running, running: false });
  const failed = await owner.post(start, {});
  assert.equal(failed.status, 503);
  assert.equal(failed.value.error, 'playground_start_failed');
  assert(!failed.body.includes(directory));
});

test('remote playground health is checked without granting local startup', async (t) => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-playground-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const keyFile = path.join(directory, 'key');
  fs.writeFileSync(keyFile, 'd'.repeat(64), { mode: 0o600 });
  const service = http.createServer((_req, res) => res.end('{"ok":true}'));
  await new Promise<void>((resolve) => service.listen(0, '127.0.0.1', resolve));
  t.after(() => {
    service.closeAllConnections();
    service.close();
  });
  const port = (service.address() as AddressInfo).port;
  const f = await fixture({
    env: {
      DISPATCH_PLAYGROUND_ORIGIN: `http://127.0.0.1:${port}`,
      DISPATCH_PLAYGROUND_KEY_FILE: keyFile,
    },
  });
  t.after(f.close);
  const owner = await f.client();
  assert.deepEqual((await owner.get('/api/platform/design-playground/status')).value, {
    configured: true,
    running: true,
    canStart: false,
  });
  assert.equal((await owner.post('/api/platform/design-playground/start', {})).status, 409);
});
