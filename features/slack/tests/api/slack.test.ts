import test from 'node:test';
import assert from 'node:assert/strict';
import { demo, fixture, until } from '../../../../core/shell/tests/support/support.js';
import { connect, event, fromSlack, install } from '../support/slack.js';

/** Where an address sends the browser, without following it. */
const goes = async (url: string) => {
  const answer = await fetch(url, { redirect: 'manual' });
  return { status: answer.status, location: answer.headers.get('location') ?? '' };
};
/** Where Slack's return sends the browser: the address with what Slack sent taken out. */
const returnedTo = async (url: string) => {
  const { location } = await goes(url);
  const [page, query] = location.split('?');
  const kept = new URLSearchParams(query);
  for (const field of ['slackState', 'slackCode', 'slackError']) kept.delete(field);
  return kept.size ? `${page}?${kept}` : page;
};
const names = (account: { workspaces: { name: string }[] }) =>
  account.workspaces.map((workspace) => workspace.name);
/** The name in Dispatch of the workspace `team`. */
const labelOf = (
  account: { workspaces: { teamId: string; label: string | null }[] },
  team: string,
) => account.workspaces.find((workspace) => workspace.teamId === team)?.label;

test('an owner adds Dispatch to several workspaces, reconnects one and disconnects one', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsps = owner.session.dsps as { id: string; name: string }[];
  const north = dsps.find((d) => d.name === 'Northline Logistics')!;
  const summit = dsps.find((d) => d.name === 'Summit Delivery')!;
  await owner.select(north.id);
  assert.deepEqual(await owner.read('/api/dsp/slack/account'), {
    available: true,
    workspaces: [],
    link: null,
  });

  // Added with a name it goes by in Dispatch.
  const started = await owner.post('/api/dsp/slack/connect', { label: ' Operations Team ' });
  assert.equal(started.status, 200, started.body);
  // Started at the admin's address, Slack sends the browser back to the DSP's Connections there.
  assert.equal(
    await returnedTo(started.value.url),
    `${f.env.DISPATCH_ORIGIN}/#dsp/${north.id}/settings?tab=connections`,
  );
  // An install belongs to the DSP it started in.
  const { state, code } = install(started.value.url);
  await owner.select(summit.id);
  const elsewhere = await owner.post('/api/dsp/slack/connect/finish', { state, code });
  assert.equal(elsewhere.value.error, 'slack_connect_expired');
  await owner.select(north.id);
  const finished = await owner.post('/api/dsp/slack/connect/finish', { state, code });
  assert.equal(finished.status, 200, finished.body);
  const [operations] = finished.value.workspaces;
  assert.equal(operations.name, 'Operations');
  assert.equal(operations.label, 'Operations Team');
  assert.equal(operations.status, 'connected');
  assert.equal(operations.needsUpdate, false);
  assert.equal(operations.byLink, false);
  // Each install is used once.
  const again = await owner.post('/api/dsp/slack/connect/finish', { state, code });
  assert.equal(again.value.error, 'slack_connect_expired');

  assert.equal((await connect(owner)).status, 200);
  assert.deepEqual(names(await owner.read('/api/dsp/slack/account')), ['Drivers', 'Operations']);
  // Reconnecting names the workspace, so Slack doesn't ask which, as fixture mode's code shows;
  // it stays one of two.
  const reconnect = await owner.post('/api/dsp/slack/connect', { team: operations.teamId });
  assert.equal(install(reconnect.value.url).code, `fixture:${operations.teamId}`);
  const reconnected = await owner.post(
    '/api/dsp/slack/connect/finish',
    install(reconnect.value.url),
  );
  assert.deepEqual(names(reconnected.value), ['Drivers', 'Operations']);
  assert.equal(labelOf(reconnected.value, operations.teamId), 'Operations Team');
  // Renamed blank, it goes by its name in Slack.
  const rename = (label: string) =>
    owner.post(`/api/dsp/slack/workspaces/${operations.teamId}/rename`, { label });
  assert.equal(labelOf((await rename('Ops')).value, operations.teamId), 'Ops');
  assert.equal(labelOf((await rename(' ')).value, operations.teamId), null);
  // Adding one the DSP has already is refused, and it stays one of two.
  const twice = await connect(owner, operations.teamId);
  assert.equal(twice.value.error, 'slack_workspace_connected');
  assert.deepEqual(names(await owner.read('/api/dsp/slack/account')), ['Drivers', 'Operations']);
  // Only one of the DSP's own workspaces is reconnected or disconnected.
  const unknown = await owner.post('/api/dsp/slack/connect', { team: 'T0SOMEONEELSE' });
  assert.equal(unknown.value.error, 'slack_workspace_not_found');

  const disconnected = await owner.post(
    `/api/dsp/slack/workspaces/${operations.teamId}/disconnect`,
  );
  assert.equal(disconnected.status, 200, disconnected.body);
  assert.deepEqual(names(disconnected.value), ['Drivers']);
});

test('a workspace belongs to one DSP, and a member connecting at its address comes back there', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client(demo.member);
  const dsps = owner.session.dsps as { id: string; name: string }[];
  const north = member.session.dsps[0] as { id: string; name: string };
  const summit = dsps.find((d) => d.id !== north.id && d.name === 'Summit Delivery')!;
  // Every permission starts off, so the owner lets the member's role manage the DSP's
  // connections, Slack's workspaces among them.
  await owner.select(north.id);
  const roles: { id: string; name: string; permissions: string[] }[] = (
    await owner.get('/api/dsp/roles')
  ).value;
  const role = roles.find((each) => each.name === 'Member')!;
  const saved = await owner.post(`/api/dsp/roles/${role.id}`, {
    name: role.name,
    permissions: [...role.permissions, 'connections.manage'],
  });
  assert.equal(saved.status, 200, saved.body);
  await member.select(north.id);

  const started = await member.post('/api/dsp/slack/connect');
  assert.equal(started.status, 200, started.body);
  const address = `http://${demo.memberDsp}.localhost:${f.env.PORT}`;
  assert.equal(await returnedTo(started.value.url), `${address}/#settings?tab=connections`);
  const finished = await member.post('/api/dsp/slack/connect/finish', install(started.value.url));
  assert.equal(finished.status, 200, finished.body);
  const [workspace] = finished.value.workspaces;

  // Another DSP is refused the workspace, and the first keeps it.
  await owner.select(summit.id);
  const taken = await connect(owner, workspace.teamId);
  assert.equal(taken.value.error, 'slack_workspace_taken');
  assert.deepEqual((await owner.read('/api/dsp/slack/account')).workspaces, []);
  assert.deepEqual(names(await member.read('/api/dsp/slack/account')), [workspace.name]);
  // Once the first lets it go, it's free.
  await member.post(`/api/dsp/slack/workspaces/${workspace.teamId}/disconnect`);
  assert.equal((await connect(owner, workspace.teamId)).status, 200);
  assert.deepEqual(names(await owner.read('/api/dsp/slack/account')), [workspace.name]);
});

test("those who manage Slack alone have Dispatch join and leave a workspace's channels", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client(demo.member);
  const north = member.session.dsps[0] as { id: string; name: string };
  await owner.select(north.id);
  const [workspace] = (await connect(owner)).value.workspaces;
  // Every permission starts off, so the owner lets the member's role manage Slack, and nothing
  // of the DSP's connections.
  const roles: { id: string; name: string; permissions: string[] }[] = (
    await owner.get('/api/dsp/roles')
  ).value;
  const role = roles.find((each) => each.name === 'Member')!;
  await owner.post(`/api/dsp/roles/${role.id}`, {
    name: role.name,
    permissions: [...role.permissions, 'slack.manage'],
  });
  await member.select(north.id);
  assert.deepEqual(names(await member.read('/api/dsp/slack/account')), [workspace.name]);

  const channels = `/api/dsp/slack/workspaces/${workspace.teamId}/channels`;
  type Channel = { id: string; name: string; private: boolean; member: boolean };
  const listed: Channel[] = (await member.read(channels)).channels;
  assert.deepEqual(
    listed.map((channel) => [channel.name, channel.private, channel.member]),
    [
      ['dispatch', false, false],
      ['general', false, true],
      ['managers', true, true],
      ['photos', false, false],
      ['random', false, false],
    ],
  );
  const id = (name: string) => listed.find((channel) => channel.name === name)!.id;
  // It joins a public channel, and leaves a private one, which then goes from the list.
  const joined = await member.post(`${channels}/${id('photos')}/join`);
  assert.equal(joined.status, 200, joined.body);
  assert.equal(joined.value.member, true);
  const left = await member.post(`${channels}/${id('managers')}/leave`);
  assert.equal(left.value.member, false);
  const now: Channel[] = (await member.read(channels)).channels;
  assert.deepEqual(
    now.filter((channel) => channel.member).map((channel) => channel.name),
    ['general', 'photos'],
  );
  assert.equal(
    now.some((channel) => channel.name === 'managers'),
    false,
  );
});

test('the install link adds one workspace, once, for whoever opens it', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = (owner.session.dsps as { id: string; name: string }[]).find(
    (d) => d.name === 'Northline Logistics',
  )!;
  await owner.select(north.id);
  const first = (await owner.post('/api/dsp/slack/link')).value;
  // A new link takes the place of the last, and gives the workspace it adds its name.
  const made = await owner.post('/api/dsp/slack/link', { label: 'Front Office' });
  assert.equal(made.status, 200, made.body);
  const link = made.value.url as string;
  assert.equal((await owner.read('/api/dsp/slack/account')).link.label, 'Front Office');
  assert.equal((await goes(first.url)).status, 404);

  // Opened without signing in: on to Slack, and back to Dispatch in Slack.
  const toSlack = await goes(link);
  assert.equal(toSlack.status, 302);
  const back = await goes(toSlack.location);
  assert.equal(back.status, 302);
  const opened = new URL(back.location);
  assert.equal(`${opened.origin}${opened.pathname}`, 'https://slack.com/app_redirect');
  const account = await owner.read('/api/dsp/slack/account');
  assert.equal(account.link, null);
  assert.deepEqual(
    account.workspaces.map((each: { name: string; label: string; byLink: boolean }) => [
      each.name,
      each.label,
      each.byLink,
    ]),
    [['Operations', 'Front Office', true]],
  );
  assert.equal(opened.searchParams.get('team'), account.workspaces[0].teamId);
  // Used once.
  assert.equal((await goes(link)).status, 404);

  // A cancelled link no longer works, nor one Slack hasn't finished with.
  const cancelled = (await owner.post('/api/dsp/slack/link')).value.url;
  const midway = await goes(cancelled);
  await owner.post('/api/dsp/slack/link/cancel');
  assert.equal((await goes(cancelled)).status, 404);
  assert.equal((await goes(midway.location)).status, 404);
  assert.equal((await owner.read('/api/dsp/slack/account')).workspaces.length, 1);
});

test("Slack's requests count only signed, and each reaches only its workspace's DSP", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsps = owner.session.dsps as { id: string; name: string }[];
  const north = dsps.find((d) => d.name === 'Northline Logistics')!;
  const summit = dsps.find((d) => d.name === 'Summit Delivery')!;
  await owner.select(north.id);
  const [northern] = (await connect(owner)).value.workspaces;
  await owner.select(summit.id);
  const [southern] = (await connect(owner)).value.workspaces;

  // Slack checks the address before sending events there.
  const challenge = JSON.stringify({ type: 'url_verification', challenge: 'c0ffee' });
  const answered = await fromSlack(f, '/slack/events', challenge);
  assert.equal(answered.status, 200);
  assert.deepEqual(await answered.json(), { challenge: 'c0ffee' });

  // Unsigned, signed otherwise, or late: refused, and nothing changes.
  const removed = event(northern.teamId, { type: 'app_uninstalled' });
  const unsigned = await fetch(`${f.env.DISPATCH_ORIGIN}/slack/events`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: removed,
  });
  assert.equal(unsigned.status, 401);
  assert.equal((await fromSlack(f, '/slack/events', removed, { secret: 'guess' })).status, 401);
  const late = Math.floor(Date.now() / 1000) - 600;
  assert.equal((await fromSlack(f, '/slack/events', removed, { at: late })).status, 401);
  const button = `payload=${encodeURIComponent(JSON.stringify({ type: 'block_actions', team: { id: northern.teamId } }))}`;
  assert.equal(
    (await fromSlack(f, '/slack/interactions', button, { secret: 'guess' })).status,
    401,
  );
  assert.equal((await fromSlack(f, '/slack/interactions', button)).status, 200);

  // Dispatch removed from the northern workspace in Slack: only Northline forgets it.
  assert.equal((await fromSlack(f, '/slack/events', removed)).status, 200);
  await owner.select(north.id);
  await until(async () => (await owner.read('/api/dsp/slack/account')).workspaces.length === 0);
  await owner.select(summit.id);
  assert.deepEqual(names(await owner.read('/api/dsp/slack/account')), [southern.name]);
  // Its tokens revoked in Slack: the workspace waits to be reconnected.
  const revoked = event(southern.teamId, {
    type: 'tokens_revoked',
    tokens: { bot: ['U0FIXTUREBOT'] },
  });
  assert.equal((await fromSlack(f, '/slack/events', revoked)).status, 200);
  await until(
    async () => (await owner.read('/api/dsp/slack/account')).workspaces[0].status === 'broken',
  );
  // Removed from Slack while Slack is switched off for the DSP: still forgotten, as it's gone.
  const switched = (enabled: boolean) =>
    owner.post(`/api/platform/dsps/${summit.id}/features`, { feature: 'slack', enabled });
  assert.equal((await switched(false)).status, 200);
  const gone = event(southern.teamId, { type: 'app_uninstalled' });
  assert.equal((await fromSlack(f, '/slack/events', gone)).status, 200);
  assert.equal((await switched(true)).status, 200);
  await owner.select(summit.id);
  await until(async () => (await owner.read('/api/dsp/slack/account')).workspaces.length === 0);

  // A workspace whose tokens Slack stopped honouring breaks as the bot next acts there, as its
  // Home tab is opened, and is whole again once reconnected.
  await owner.select(north.id);
  const [lapsed] = (await connect(owner, 'T0GONE1')).value.workspaces;
  const opened = event(lapsed.teamId, { type: 'app_home_opened', tab: 'home', user: 'U0OWNER' });
  assert.equal((await fromSlack(f, '/slack/events', opened)).status, 200);
  await until(
    async () => (await owner.read('/api/dsp/slack/account')).workspaces[0].status === 'broken',
  );
  const reconnect = await owner.post('/api/dsp/slack/connect', { team: lapsed.teamId });
  const reconnected = await owner.post(
    '/api/dsp/slack/connect/finish',
    install(reconnect.value.url),
  );
  assert.equal(reconnected.value.workspaces[0].status, 'connected');
});
