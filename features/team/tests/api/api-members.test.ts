import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { capturedMail } from '../../../../core/shell/tests/support/mail-support.js';
import { demo, fixture } from '../../../../core/shell/tests/support/support.js';
const { password } = demo;

test('Rust provisioning, invitation acceptance, profile setup, removal and restoration preserve boundaries', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const created = await owner.post('/api/platform/dsps', { ownerEmail: 'new@dispatch.test' });
  assert.equal(created.status, 201);
  const id = created.value.dsp.id;
  for (const area of ['config', 'data', 'secrets', 'state'])
    assert.equal(fs.statSync(path.join(f.root, 'dsps', id, area)).mode & 0o077, 0);
  assert.equal(created.value.invitationUrl, undefined);
  assert.deepEqual(created.value.invitation, { email: 'new@dispatch.test', status: 'queued' });
  const message = await capturedMail(f.root, 'new@dispatch.test');
  assert.match(message.subject, /^\[Dispatch Dev\]/);
  assert.match(message.html, />Start DSP onboarding<\/a>/);
  assert.match(message.text, / invited you to set up a new DSP on Dispatch as its owner\./);
  assert.equal(message.origin, f.env.DISPATCH_ORIGIN);
  const raw = /token=([A-Za-z0-9_-]{43})/.exec(message.text)![1];
  // A new DSP's first owner sets it up at the invite page, and its short code gives it its
  // own address.
  assert.match(
    message.text,
    new RegExp(`http://invite\\.localhost:${f.env.PORT}/#invite\\?token=`),
  );
  const page = f.at('invite');
  const at = f.at('new');
  const invite = `/api/invitations/${raw}`;
  assert.equal((await f.request(invite, undefined, page)).value.email, 'new@dispatch.test');
  const setup = {
    name: 'New Logistics',
    timezone: 'America/Chicago',
    abbreviation: 'NEW',
    stationCode: 'dtx1',
  };
  const accepted = await f.request(
    invite + '/accept',
    { firstName: 'New', lastName: 'Owner', password, dspProfile: setup },
    page,
  );
  assert.equal(accepted.status, 200, accepted.body);
  assert.equal(accepted.value.signIn, `http://new.localhost:${f.env.PORT}/#signin`);
  assert.equal(
    (
      await f.request(
        invite + '/accept',
        { firstName: 'Replay', lastName: 'Owner', password, dspProfile: setup },
        page,
      )
    ).status,
    404,
  );
  // Opening a used link again says who it let in, not that it expired, and where to sign in.
  assert.deepEqual((await f.request(invite, undefined, page)).value, {
    accepted: true,
    email: 'new@dispatch.test',
    dspId: id,
    dspName: 'New Logistics',
    role: 'Owner',
    signIn: accepted.value.signIn,
  });
  const user = await f.client('new@dispatch.test', password, 'new');
  const view = await user.select(id);
  assert.equal(view.dsp.code, 'new');
  assert.deepEqual(
    [view.profile.abbreviation, view.profile.stationCode, view.profile.setupRequired],
    ['NEW', 'DTX1', false],
  );
  const members = (await user.get('/api/dsp/members')).value;
  assert.equal((await user.post(`/api/dsp/members/${members[0].id}`, { role: null })).status, 409);
  assert.equal((await owner.post(`/api/platform/dsps/${id}/remove`)).status, 200);
  assert.equal((await f.request(invite, undefined, at)).status, 404);
  assert.equal((await user.get('/api/dsp/members')).status, 409);
  assert.equal(
    (await owner.post(`/api/platform/dsps/${id}/status`, { status: 'active' })).status,
    409,
  );
  assert.equal((await owner.post(`/api/platform/dsps/${id}/restore`)).status, 200);
  await user.select(id);
  assert.equal((await f.request(invite, undefined, at)).value.accepted, true);
  // After the invitation's own seven days, a used link reads as expired like any other.
  f.database('data/platform/accounts.sqlite', (db) =>
    db.prepare("UPDATE invitations SET expires_at=0 WHERE email='new@dispatch.test'").run(),
  );
  assert.equal((await f.request(invite, undefined, at)).value.error, 'invitation_expired');
  const dev = owner.session.dsps.find((d: { permanent: boolean }) => d.permanent);
  assert.equal((await owner.post(`/api/platform/dsps/${dev.id}/remove`)).status, 409);
  assert.equal(
    (await owner.post(`/api/platform/dsps/${dev.id}/status`, { status: 'suspended' })).status,
    409,
  );
});

test('existing-account invitations require the account password and revocation respects tenant ownership', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  const dev = owner.session.dsps.find(
    (d: { permanent: boolean; id: string }) =>
      !d.permanent && !member.session.dsps.some((mine: { id: string }) => mine.id === d.id),
  );
  await owner.select(dev.id);
  const roles: { id: string; name: string }[] = (await owner.get('/api/dsp/roles')).value;
  const roleId = (name: string) => roles.find((role) => role.name === name)!.id;
  const invited = await owner.post('/api/dsp/members/invite', {
    email: 'member@dispatch.test',
    role: roleId('Manager'),
  });
  assert.equal(invited.status, 200);
  assert.equal(invited.value.invitationUrl, undefined);
  const token = /token=([A-Za-z0-9_-]{43})/.exec(
    (await capturedMail(f.root, 'member@dispatch.test')).text,
  )![1];
  const at = f.at(dev.code);
  const accept = (secret: string) =>
    f.request(
      `/api/invitations/${token}/accept`,
      { firstName: 'Fake', lastName: 'Owner', password: secret },
      at,
    );
  assert.equal((await accept('wrong-password-long')).status, 403);
  assert.equal((await accept(password)).status, 200);
  const existing = await f.client('member@dispatch.test', password, dev.code);
  assert.equal(existing.session.user.firstName, 'Jordan');
  await existing.select(dev.id);
  assert.equal(
    (await existing.post('/api/dsp/invitations/revoke', { email: 'new@dispatch.test' })).status,
    403,
  );
  const pending = await owner.post('/api/dsp/members/invite', {
    email: 'new@dispatch.test',
    role: roleId('Member'),
  });
  assert.equal(pending.value.invitationUrl, undefined);
  const raw = /token=([A-Za-z0-9_-]{43})/.exec(
    (await capturedMail(f.root, 'new@dispatch.test')).text,
  )![1];
  assert.equal(
    (await member.post('/api/dsp/invitations/revoke', { email: 'new@dispatch.test' })).status,
    403,
  );
  assert.equal(
    (await owner.post('/api/dsp/invitations/revoke', { email: 'new@dispatch.test' })).status,
    200,
  );
  assert.equal((await f.request(`/api/invitations/${raw}`, undefined, at)).status, 404);
});

test('owner onboarding validates DSP setup before accepting and denies setup through member invites', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const platform = await f.client();
  const created = await platform.post('/api/platform/dsps', { ownerEmail: 'setup@dispatch.test' });
  const id = created.value.dsp.id;
  const token = /token=([A-Za-z0-9_-]{43})/.exec(
    (await capturedMail(f.root, 'setup@dispatch.test')).text,
  )![1];
  const invite = `/api/invitations/${token}`;
  const page = f.at('invite');
  const account = { firstName: 'Setup', lastName: 'Owner', password };
  const dspProfile = {
    name: 'Northstar Logistics',
    abbreviation: 'NSTL',
    stationCode: 'tst1',
    timezone: 'America/Los_Angeles',
  };
  // The link opens at the invite page, never at the admin's or another DSP's address.
  assert.equal((await f.request(invite)).status, 404);
  assert.equal((await f.request(invite, undefined, f.at(demo.memberDsp))).status, 404);
  // A short code is checked as it is typed: one another DSP has, or a reserved name, is taken.
  const check = async (code: string) =>
    (await f.request(`${invite}/short-code?code=${code}`, undefined, page)).value.available;
  assert.deepEqual(
    [await check('NSTL'), await check(demo.memberDsp), await check('admin')],
    [true, false, false],
  );
  for (const [invalid, error] of [
    [{ abbreviation: '' }, 'invalid_input'],
    [{ abbreviation: '   ' }, 'invalid_short_code'],
    [{ abbreviation: 'N.S.T.L' }, 'invalid_short_code'],
    [{ stationCode: '!!!' }, 'invalid_input'],
    [{ timezone: 'not/a-timezone' }, 'invalid_timezone'],
  ] as const) {
    const refused = await f.request(
      invite + '/accept',
      { ...account, dspProfile: { ...dspProfile, ...invalid } },
      page,
    );
    assert.deepEqual([refused.status, refused.value.error], [400, error]);
    assert.equal((await f.request(invite, undefined, page)).value.onboarding, true);
  }
  for (const [profile, error] of [
    [undefined, 'dsp_setup_required'],
    [{ ...dspProfile, abbreviation: demo.memberDsp.toUpperCase() }, 'short_code_taken'],
  ] as const) {
    const refused = await f.request(invite + '/accept', { ...account, dspProfile: profile }, page);
    assert.equal(refused.value.error, error);
  }
  assert.equal((await f.request(invite + '/accept', { ...account, dspProfile }, page)).status, 200);
  const user = await f.client('setup@dispatch.test', password, 'nstl');
  const view = await user.select(id);
  assert.equal(view.dsp.name, 'Northstar Logistics');
  assert.equal(view.dsp.timezone, 'America/Los_Angeles');
  assert.deepEqual(
    [view.profile.abbreviation, view.profile.stationCode, view.profile.setupRequired],
    ['NSTL', 'TST1', false],
  );
  assert.equal((await f.request(invite + '/accept', { ...account, dspProfile }, page)).status, 404);
  assert.equal(
    (await user.post('/api/dsp/profile', { ...dspProfile, abbreviation: ' ' })).status,
    400,
  );
  // Once set, the short code stays: the DSP's own settings never move its address.
  assert.equal(
    (await user.post('/api/dsp/profile', { ...dspProfile, abbreviation: 'OTHER' })).value.error,
    'short_code_locked',
  );
  const roles: { id: string; name: string }[] = (await user.get('/api/dsp/roles')).value;
  await user.post('/api/dsp/members/invite', {
    email: 'member-setup@dispatch.test',
    role: roles.find((r) => r.name === 'Member')!.id,
  });
  const memberToken = /token=([A-Za-z0-9_-]{43})/.exec(
    (await capturedMail(f.root, 'member-setup@dispatch.test')).text,
  )![1];
  const memberInvite = `/api/invitations/${memberToken}`;
  const at = f.at('nstl');
  const details = await f.request(memberInvite, undefined, at);
  assert.equal(details.value.onboarding, false);
  assert.equal(details.value.stationCode, 'TST1');
  assert.equal(details.value.timezone, 'America/Los_Angeles');
  assert.equal(
    (await f.request(memberInvite + '/accept', { ...account, dspProfile }, at)).status,
    403,
  );
  assert.equal((await f.request(memberInvite, undefined, at)).status, 200);
  assert.equal((await f.request(memberInvite + '/accept', account, at)).status, 200);
  assert.equal((await user.select(id)).dsp.name, 'Northstar Logistics');
});
