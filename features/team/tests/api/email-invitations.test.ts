import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../../core/shell/tests/support/support.js';
import { capturedMail } from '../../../../core/shell/tests/support/mail-support.js';

const token = 'synthetic-worker-secret-for-invite-tests';

test('Dev invitations and production use isolated configuration, state and accounts', async (t) => {
  const dev = await fixture(false);
  t.after(dev.close);
  const production = await fixture({
    seed: false,
    env: {
      DISPATCH_ENVIRONMENT: 'production',
      NODE_ENV: 'production',
      DISPATCH_PROVIDER_MODE: 'native',
      DISPATCH_ORIGIN: 'https://production.dispatch.test',
      DISPATCH_DEV_MAIL_MODE: 'disabled',
      DISPATCH_PRODUCTION_MAIL_MODE: 'disabled',
    },
  });
  t.after(production.close);
  const devOwner = await dev.client();
  const productionOwner = await production.client();
  assert.equal(devOwner.session.dsps.length, 1);
  assert.equal(devOwner.session.dsps[0].name, 'Dev DSP');
  assert.deepEqual(productionOwner.session.dsps, []);
  assert.deepEqual((await productionOwner.get('/api/platform/dsps')).value, []);
  assert.equal(productionOwner.session.user.platformOwner, true);
  assert.equal((await productionOwner.get('/api/platform/dsps')).status, 200);
  assert.equal(
    (await productionOwner.post('/api/platform/dsps', { ownerEmail: 'new-owner@dispatch.test' }))
      .status,
    503,
  );
  assert.deepEqual((await productionOwner.get('/api/platform/dsps')).value, []);
  const result = await devOwner.post('/api/platform/dsps', {
    ownerEmail: 'new-owner@dispatch.test',
  });
  assert.equal(result.status, 201);
  assert.equal(result.value.invitationUrl, undefined);
  const message = await capturedMail(dev.root, 'new-owner@dispatch.test');
  assert(message.text.includes(dev.env.DISPATCH_ORIGIN));
  assert.equal(message.subject, '[Dispatch Dev] Set up your DSP on Dispatch');
  assert.match(message.html, />Start DSP onboarding<\/a>/);
  const raw = /token=([A-Za-z0-9_-]{43})/.exec(message.text)![1];
  assert.equal((await production.request(`/api/invitations/${raw}`)).status, 404);
  assert.equal((await dev.request(`/api/invitations/${raw}`)).value.onboarding, true);
  dev.database('data/platform/accounts.sqlite', (db) =>
    db.prepare('UPDATE invitations SET expires_at=0').run(),
  );
  assert.equal(
    (
      await dev.request(`/api/invitations/${raw}/accept`, {
        firstName: 'New',
        lastName: 'Owner',
        password: 'An-example-password!',
      })
    ).status,
    404,
  );
});

test('disabled Dev mail does not fall back to production mail or create a DSP/invitation', async (t) => {
  const f = await fixture({
    seed: false,
    env: {
      DISPATCH_DEV_MAIL_MODE: 'disabled',
      DISPATCH_PRODUCTION_MAIL_MODE: 'cloudflare',
      DISPATCH_PRODUCTION_MAIL_WORKER_URL: 'https://unused.example/send',
      DISPATCH_PRODUCTION_MAIL_WORKER_TOKEN: token,
    },
  });
  t.after(f.close);
  const owner = await f.client();
  assert.equal(
    (await owner.post('/api/platform/dsps', { ownerEmail: 'never@dispatch.test' })).status,
    503,
  );
  assert.equal((await owner.get('/api/platform/dsps')).value.length, 1);
  await owner.select(owner.session.dsps[0].id);
  const role = (await owner.get('/api/dsp/roles')).value.find(
    (item: { name: string }) => item.name === 'Member',
  );
  assert.equal(
    (await owner.post('/api/dsp/members/invite', { email: 'never@dispatch.test', role: role.id }))
      .status,
    503,
  );
  assert.equal((await owner.get('/api/dsp/invitations')).value.length, 0);
});
