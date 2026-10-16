import test from 'node:test';
import assert from 'node:assert/strict';
import type { DriverMatch } from '../../../features/driver_match/api/index.js';
import { connect, event, fromSlack } from '../../../features/slack/tests/support/slack.js';
import { fixture, until } from '../../../core/shell/tests/support/support.js';

test("a workspace's members reach Driver Match once it's connected, and whoever joins at once", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  const [drivers] = (await connect(owner, 'T0FIXTURE2')).value.workspaces;
  const people = () => owner.read('/api/dsp/driver-match') as Promise<DriverMatch>;
  const holder = (result: DriverMatch, member: string) =>
    result.drivers.find((d) => d.ids.some((i) => i.source === 'slack' && i.id === member));
  await until(async () => (await people()).sources.includes('slack'));
  // Someone only Slack knows, and deactivated there, has left.
  assert.equal(holder(await people(), `${drivers.teamId}:U0FIXGONE`)?.role, 'former');
  // Someone joining the workspace is read as Slack tells of them, and works in the office.
  const joined = event(drivers.teamId, {
    type: 'team_join',
    user: {
      id: 'U0FIXNEW',
      profile: { real_name: 'Riley Stone', email: 'riley.stone@example.com' },
    },
  });
  assert.equal((await fromSlack(f, '/slack/events', joined)).status, 200);
  await until(async () => holder(await people(), `${drivers.teamId}:U0FIXNEW`)?.role === 'office');
});
