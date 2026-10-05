import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { demo, fixture, prepare } from '../../../shell/tests/support/support.js';

test('trusted tunnel clients have separate IP allowances and retain account throttling', async (t) => {
  const f = await fixture({ env: { DISPATCH_TRUSTED_PROXY: 'cloudflare' } });
  t.after(f.close);
  const full = (key: string, count: number) =>
    f.database('data/platform/accounts.sqlite', (db) =>
      db
        .prepare(
          "INSERT OR REPLACE INTO throttle(key,count,reset_at,namespace) VALUES (?,?,?,'login-ip')",
        )
        .run(createHash('sha256').update(key).digest('hex'), count, Date.now() + 900000),
    );
  full('login-ip:203.0.113.1', 30);
  const body = { email: 'owner@dispatch.test', password: demo.password };
  const login = (ip: string) => f.request('/api/auth/login', body, { 'cf-connecting-ip': ip });
  assert.equal((await login('203.0.113.1')).status, 429);
  assert.equal((await login('203.0.113.2')).status, 200);
  full('login:email:owner@dispatch.test', 10);
  assert.equal((await login('203.0.113.3')).status, 429);
  assert.equal((await login('203.0.113.3, 203.0.113.4')).status, 400);
});

test('untrusted forwarding headers cannot bypass the peer throttle', async (t) => {
  const f = await fixture({ env: { DISPATCH_TRUSTED_PROXY: 'none' } });
  t.after(f.close);
  f.database('data/platform/accounts.sqlite', (db) =>
    db
      .prepare("INSERT INTO throttle(key,count,reset_at,namespace) VALUES (?,?,?,'login-ip')")
      .run(
        createHash('sha256').update('login-ip:127.0.0.1').digest('hex'),
        30,
        Date.now() + 900000,
      ),
  );
  const result = await f.request(
    '/api/auth/login',
    { email: 'owner@dispatch.test', password: demo.password },
    {
      'cf-connecting-ip': '203.0.113.1',
      'x-forwarded-for': '203.0.113.2',
      forwarded: 'for=203.0.113.3',
    },
  );
  assert.equal(result.status, 429);
});

test('high-cardinality throttle input evicts noisy keys instead of exhausting every login', async (t) => {
  const f = await fixture({ env: { DISPATCH_TRUSTED_PROXY: 'cloudflare' } });
  t.after(f.close);
  f.database('data/platform/accounts.sqlite', (db) => {
    db.exec('BEGIN IMMEDIATE');
    const insert = db.prepare(
      "INSERT INTO throttle(key,count,reset_at,namespace) VALUES (?,1,?,'login-ip')",
    );
    for (let index = 0; index < 4096; index++)
      insert.run(createHash('sha256').update(`noise-${index}`).digest('hex'), Date.now() + 900_000);
    db.exec('COMMIT');
  });
  const result = await f.request(
    '/api/auth/login',
    { email: demo.email, password: demo.password },
    { 'cf-connecting-ip': '2606:4700:abcd:1::1' },
  );
  assert.equal(result.status, 200);
  assert.equal(
    f.database(
      'data/platform/accounts.sqlite',
      (db) =>
        (
          db.prepare("SELECT count(*) n FROM throttle WHERE namespace='login-ip'").get() as {
            n: number;
          }
        ).n,
    ),
    4096,
  );
});

test('production rejects conflicting modes and unsafe defaults before opening storage', async (t) => {
  const f = await prepare(false);
  t.after(f.cleanup);
  const binary = path.resolve(process.env.DISPATCH_TEST_BINARY ?? 'target/debug/dispatch-backend');
  const production = {
    ...f.env,
    NODE_ENV: undefined,
    DISPATCH_ENVIRONMENT: 'production',
    DISPATCH_ORIGIN: 'https://dispatch.example',
    DISPATCH_PROVIDER_MODE: 'native',
    DISPATCH_PRODUCTION_MAIL_MODE: 'disabled',
  };
  const status = (env: NodeJS.ProcessEnv) =>
    spawnSync(binary, ['status'], { env, encoding: 'utf8' });
  assert.equal(
    status(production).status,
    0,
    'Production must default to production behavior without NODE_ENV',
  );
  for (const overrides of [
    { NODE_ENV: 'development' },
    { NODE_ENV: 'test' },
    { DISPATCH_PROVIDER_MODE: 'fixture' },
    { DISPATCH_PRODUCTION_MAIL_MODE: 'capture' },
    { DISPATCH_ORIGIN: 'http://dispatch.example' },
    { DISPATCH_STATE_ROOT: undefined },
    { DISPATCH_ORIGIN: undefined },
  ])
    assert.notEqual(status({ ...production, ...overrides }).status, 0, JSON.stringify(overrides));
  assert.match(status({ ...f.env, NODE_ENV: 'typo' }).stderr, /invalid_runtime_mode/);
  assert.match(
    status({ ...f.env, DISPATCH_PROVIDER_MODE: 'typo' }).stderr,
    /invalid_provider_mode/,
  );
  assert.match(status({ ...f.env, DISPATCH_TRUSTED_PROXY: 'all' }).stderr, /invalid_trusted_proxy/);
});
