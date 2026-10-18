// What Slack's integrations keep in Documents, through the `files` contract it exposes: Import Files' files,
// and Sync Canvases' canvases, as Google Docs.
import test from 'node:test';
import assert from 'node:assert/strict';
import { connect, event, fromSlack } from '../../../features/slack/tests/support/slack.js';
import { demo, fixture, until } from '../../../core/shell/tests/support/support.js';

type Client = Awaited<ReturnType<Awaited<ReturnType<typeof fixture>>['client']>>;

/** Connects the DSP open in `client` to fixture mode's Google account, for Documents. */
async function connectDrive(client: Client) {
  const started = await client.post('/api/dsp/documents/connect');
  const sent = new URL(started.value.url).searchParams;
  const finished = await client.post('/api/dsp/documents/connect/finish', {
    state: sent.get('state'),
    code: sent.get('code'),
  });
  assert.equal(finished.status, 200, finished.body);
}

test('a file posted in a linked Slack channel is saved in Documents once, named for who posted it', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  await connectDrive(owner);
  const [workspace] = (await connect(owner, 'T0FIXTURE1')).value.workspaces;
  const at = `/api/dsp/slack/workspaces/${workspace.teamId}`;
  const imports = `${at}/imports`;

  // Off in the workspace, Import Files has nothing to show.
  assert.equal((await owner.get(imports)).status, 409);
  const on = await owner.post(`${at}/integrations/slack.import_files`, { on: true });
  assert.equal(on.status, 200, on.body);
  assert.deepEqual((await owner.read(imports)).links, []);

  // Only a channel Dispatch is in links; #photos, once joined, links to a new folder named
  // after it.
  const notIn = await owner.post(`${imports}/links`, { channel: 'C0FIXPHOTOS' });
  assert.equal(notIn.value.error, 'slack_channel_not_joined');
  await owner.post(`${at}/channels/C0FIXPHOTOS/join`);
  const linked = await owner.post(`${imports}/links`, { channel: 'C0FIXPHOTOS' });
  assert.equal(linked.status, 200, linked.body);
  const [photos] = linked.value.links;
  assert.equal(photos.folder.name, 'photos');

  // Slack tells of the van's photo shared there, twice, as it does a file shared twice: it's
  // saved once, in that folder, added by the name Slack shows its poster by.
  const shared = () =>
    fromSlack(
      f,
      '/slack/events',
      event(workspace.teamId, {
        type: 'file_shared',
        file_id: 'F0FIXVAN',
        channel_id: 'C0FIXPHOTOS',
        user_id: 'U0FIXOWNER',
        event_ts: (Date.now() / 1000).toFixed(6),
      }),
    );
  assert.equal((await shared()).status, 200);
  await until(async () =>
    (await owner.read(imports)).recent.some((file: { status: string }) => file.status === 'saved'),
  );
  assert.equal((await shared()).status, 200);
  const folder = await owner.read(`/api/dsp/documents/folder?id=${photos.folder.id}`);
  assert.deepEqual(
    folder.items.map((item: { name: string; addedBy: string }) => [item.name, item.addedBy]),
    [['Van 12 damage.jpg', 'Morgan Lane']],
  );
  const [saved] = (await owner.read(imports)).recent;
  assert.equal(saved.postedBy, 'Morgan Lane');
  assert.equal(saved.channelName, 'photos');

  // Someone who manages Slack but can't see Documents' folders doesn't reach Import Files.
  const member = await f.client(demo.member);
  const roles: { id: string; name: string; permissions: string[] }[] = (
    await owner.get('/api/dsp/roles')
  ).value;
  const role = roles.find((each) => each.name === 'Member')!;
  await owner.post(`/api/dsp/roles/${role.id}`, {
    name: role.name,
    permissions: [...role.permissions.filter((p) => p !== 'documents.use'), 'slack.manage'],
  });
  await member.select(north.id);
  assert.equal((await member.get(imports)).value.error, 'slack_files_forbidden');
});

test("a linked channel's canvas is kept in Documents as a Google Doc, once, however it's chosen", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  await connectDrive(owner);
  const [workspace] = (await connect(owner, 'T0FIXTURE1')).value.workspaces;
  const at = `/api/dsp/slack/workspaces/${workspace.teamId}`;
  const canvases = `${at}/canvases`;
  assert.equal(
    (await owner.post(`${at}/integrations/slack.sync_canvases`, { on: true })).status,
    200,
  );
  await owner.post(`${at}/channels/C0FIXPHOTOS/join`);

  // Linking #photos keeps its canvas in a new folder named after it, as a Doc named for the
  // canvas and added by the member who made it.
  const linked = await owner.post(`${canvases}/links`, { channel: 'C0FIXPHOTOS' });
  assert.equal(linked.status, 200, linked.body);
  const [photos] = linked.value.links;
  const canvas = async () => (await owner.read(canvases)).canvases[0];
  await until(async () => (await canvas())?.status === 'synced');
  const kept = await canvas();
  assert.equal(kept.title, 'Write-ups');
  const folder = await owner.read(`/api/dsp/documents/folder?id=${photos.folder.id}`);
  assert.deepEqual(
    folder.items.map((item: { name: string; kind: string; addedBy: string }) => [
      item.name,
      item.kind,
      item.addedBy,
    ]),
    [['Write-ups', 'doc', 'Morgan Lane']],
  );

  // Unlinked, it's no longer kept; chosen on its own, it's kept again as the same Doc, not
  // twice.
  await owner.post(`${canvases}/links/C0FIXPHOTOS/remove`);
  assert.deepEqual((await owner.read(canvases)).canvases, []);
  const chooser = await owner.read(`${canvases}/channels/C0FIXPHOTOS`);
  assert.deepEqual(chooser, {
    linked: false,
    canvases: [{ id: 'F0FIXCANVAS', title: 'Write-ups', chosen: false }],
  });
  const chosen = await owner.post(`${canvases}/chosen`, {
    channel: 'C0FIXPHOTOS',
    canvases: ['F0FIXCANVAS'],
    folder: photos.folder.id,
  });
  assert.equal(chosen.status, 200, chosen.body);
  const again = await canvas();
  assert.deepEqual([again.status, again.link, again.folder.name], ['synced', kept.link, 'photos']);
  const after = await owner.read(`/api/dsp/documents/folder?id=${photos.folder.id}`);
  assert.equal(after.items.length, 1);

  // No longer chosen, it isn't synced; its Doc stays.
  await owner.post(`${canvases}/chosen/F0FIXCANVAS/remove`);
  assert.deepEqual((await owner.read(canvases)).canvases, []);
  assert.equal(
    (await owner.read(`/api/dsp/documents/folder?id=${photos.folder.id}`)).items.length,
    1,
  );
});
