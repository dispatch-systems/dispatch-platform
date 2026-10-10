import crypto from 'node:crypto';
import type { Page } from '@playwright/test';
import type { AgentKeys } from '../../../mcp/api/index.js';
import { platformHash } from '../../../core/shell/frontend/runtime/navigation.js';
import { utcDay } from '../../../core/shell/frontend/lib/format.js';
import { test, expect, login } from '../../../core/shell/tests/support/fixtures.js';
import { startingTools } from '../support/agent-keys.js';

// The Agents page: keys, connected apps and what they called.

/** Connects Claude Code as `name` while apps may connect: it asks from this browser, the owner
 * signed in on `page` approves it, and the app redeems its code. Its listener on this computer
 * answers for it. */
async function connectApp(page: Page, origin: string, name: string) {
  const callback = 'http://localhost:43821/callback';
  const client = 'https://claude.ai/oauth/claude-code-client-metadata';
  const resource = `${origin}/api/v1/mcp`;
  const verifier = crypto.randomBytes(32).toString('base64url');
  await page.route('http://localhost:43821/**', (route) =>
    route.fulfill({ contentType: 'text/html', body: '<p>Authentication complete.</p>' }),
  );
  const query = new URLSearchParams({
    response_type: 'code',
    client_id: client,
    redirect_uri: callback,
    code_challenge: crypto.createHash('sha256').update(verifier).digest('base64url'),
    code_challenge_method: 'S256',
    state: 'connect',
    resource,
  });
  await page.goto(`/oauth/authorize?${query}`);
  const approval = page.getByRole('form', { name: 'Claude Code' });
  await approval.getByLabel('Connection name').fill(name);
  await approval.getByRole('button', { name: 'Approve', exact: true }).click();
  await page.waitForURL((url) => url.href.startsWith(`${callback}?`));
  const token = await page.request.post('/oauth/token', {
    form: {
      grant_type: 'authorization_code',
      code: new URL(page.url()).searchParams.get('code')!,
      redirect_uri: callback,
      code_verifier: verifier,
      client_id: client,
      resource,
    },
  });
  expect(token.status(), await token.text()).toBe(200);
}

test('the platform owner makes a key, sees it once, tests it, changes and revokes it', async ({
  page,
  baseURL,
  dispatch,
}) => {
  const starting = await startingTools(dispatch);
  await login(page);
  await page.getByRole('link', { name: 'Agents', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Agents', level: 1 })).toBeVisible();
  // Apps come first; keys have a tab of their own.
  await expect(page.getByRole('tablist', { name: 'Agents' }).getByRole('tab')).toHaveText([
    'Apps',
    'Activity',
    'Keys',
  ]);
  await expect(page.getByRole('tab', { name: 'Apps', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await page.getByRole('tab', { name: 'Keys', exact: true }).click();
  await expect(page.getByText('No keys yet')).toBeVisible();

  await page.getByRole('button', { name: 'New key', exact: true }).click();
  const sheet = page.getByRole('dialog', { name: 'New key' });
  await sheet.getByLabel('Name').fill('Laptop – Claude Code');
  await sheet.getByText('Choose DSPs', { exact: true }).click();
  await sheet.getByRole('checkbox', { name: 'Northline Logistics' }).check();
  await expect(sheet.getByRole('radio', { name: /Operator/ })).toHaveCount(0);
  await sheet.getByRole('button', { name: 'Create key', exact: true }).click();

  // The key is shown this once, with its setup and a working test.
  const ready = page.getByRole('dialog', { name: 'Laptop – Claude Code is ready' });
  const token = await ready.locator('.agents-key code').textContent();
  expect(token).toMatch(/^dsk_dev_[0-9A-Za-z]{38}$/);
  // Each agent's own setup, starting with Claude Code's.
  const setup = ready.locator('.agents-snippet');
  await expect(setup).toContainText(`DISPATCH_KEY="${token}"`);
  await expect(setup).toContainText('claude mcp add --transport http --scope user dispatch');
  await ready.getByRole('tab', { name: 'Codex', exact: true }).click();
  await expect(setup).toContainText('codex mcp add dispatch --url');
  await expect(setup).toContainText('/api/v1/mcp');
  await ready.getByRole('tab', { name: 'Other MCP apps', exact: true }).click();
  await expect(setup).toContainText(`"Authorization": "Bearer ${token}"`);
  await ready.getByRole('tab', { name: 'Claude Code', exact: true }).click();
  await ready.getByRole('button', { name: 'Send test request' }).click();
  await expect(ready.getByRole('status')).toContainText('Connected · 1 DSP');
  await ready.getByRole('button', { name: 'Done', exact: true }).click();

  const row = page.getByRole('row').filter({ hasText: 'Laptop – Claude Code' });
  await expect(row).toContainText(`…${token!.slice(-4)}`);
  await expect(row).toContainText(starting.text);
  await expect(row).toContainText('Northline Logistics');
  // The page never shows the key again.
  await expect(page.getByText(token!)).toHaveCount(0);

  // A changed key asks before its edits are thrown away.
  await row.getByRole('button', { name: 'Open Laptop – Claude Code' }).click();
  const edit = page.getByRole('dialog', { name: 'Laptop – Claude Code' });
  await edit.getByRole('combobox', { name: /^Expires/ }).selectOption('30');
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Save changes', exact: true }).last().click();
  await expect(row).toContainText(starting.text);

  // Using keys, folded away beneath them: testing a key answers as an agent would.
  const using = page.getByRole('group').filter({ hasText: 'Using keys' });
  await expect(using.locator('summary')).toHaveText('Using keys· addresses and key tester');
  await expect(using.getByLabel('Key to test')).toBeHidden();
  await using.locator('summary').click();
  await expect(using.locator('summary')).toHaveText('Using keys');
  await using.getByLabel('Key to test').fill(token!);
  await using.getByRole('button', { name: 'Test key', exact: true }).click();
  await expect(using.getByRole('status')).toContainText('Connected · 1 DSP');
  // The addresses an agent needs, ready to copy.
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write'], { origin: baseURL });
  const addresses = using.locator('.agents-endpoint');
  await expect(addresses).toHaveText([
    `MCP${baseURL}/api/v1/mcpCopy`,
    `REST API${baseURL}/api/v1Copy`,
  ]);
  await using.getByRole('button', { name: 'Copy API address', exact: true }).click();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(`${baseURL}/api/v1`);
  // Each new key comes with its own setup, so the page holds none.
  await expect(page.locator('.agents-snippet')).toHaveCount(0);

  // Revoked, it stops at once.
  await row.getByRole('button', { name: 'Open Laptop – Claude Code' }).click();
  await page.getByRole('button', { name: 'Revoke key', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'Revoke Laptop – Claude Code?' })
    .getByRole('button', { name: 'Revoke key', exact: true })
    .click();
  await expect(page.getByRole('button', { name: 'Show 1 revoked or expired key' })).toBeVisible();
  await using.getByLabel('Key to test').fill(token!);
  await using.getByRole('button', { name: 'Test key', exact: true }).click();
  await expect(using.getByRole('status')).toContainText('That key was revoked.');
});

test('a key takes tools, to read, as they are added, until the owner says otherwise', async ({
  page,
  dispatch,
}) => {
  const starting = await startingTools(dispatch);
  await login(page);
  await page.getByRole('link', { name: 'Agents', exact: true }).click();
  await page.getByRole('tab', { name: 'Keys', exact: true }).click();
  await page.getByRole('button', { name: 'New key', exact: true }).click();
  const sheet = page.getByRole('dialog', { name: 'New key' });
  await sheet.getByLabel('Name').fill('Nightly report script');
  const tools = sheet.getByRole('group', { name: 'Tools', exact: true });
  const added = tools.getByRole('switch', { name: 'New tools, to read', exact: true });
  await expect(added).toBeChecked();
  await sheet.getByRole('button', { name: 'Create key', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'Nightly report script is ready' })
    .getByRole('button', { name: 'Done', exact: true })
    .click();
  const row = page.getByRole('row').filter({ hasText: 'Nightly report script' });
  await expect(row).toContainText(starting.text);

  // Changed afterwards, without making the key again.
  await row.getByRole('button', { name: 'Open Nightly report script' }).click();
  const edit = page.getByRole('dialog', { name: 'Nightly report script', exact: true });
  await edit.getByRole('switch', { name: 'New tools, to read', exact: true }).click();
  await edit.getByRole('button', { name: 'Save changes', exact: true }).click();
  // With tools to choose, the key keeps them; it only stops taking those added later.
  await expect(row).toContainText(starting.count ? starting.text : 'No tools');
});

test('the Activity tab lists each call a key makes, and what Dispatch answered', async ({
  page,
  dispatch,
}) => {
  await login(page);
  await page.getByRole('link', { name: 'Agents', exact: true }).click();
  await page.getByRole('tab', { name: 'Activity', exact: true }).click();
  await expect(page.getByText('No agent calls yet')).toBeVisible();

  const owner = await dispatch.client();
  const made = await owner.post('/api/platform/agents/keys', {
    name: 'Nightly report script',
    allDsps: true,
    dsps: [],
    access: 'read',
    allTools: true,
    tools: {},
    expiresAt: null,
  });
  expect(made.status, made.body).toBe(200);
  const call = await dispatch.request('/api/v1/whoami', undefined, {
    authorization: `Bearer ${made.value.token}`,
  });
  expect(call.status, call.body).toBe(200);

  // Calls are written a moment after they're made; the tab reads them when it opens.
  const row = page.getByRole('row').filter({ hasText: 'Nightly report script' });
  await expect(async () => {
    await page.getByRole('tab', { name: 'Keys', exact: true }).click();
    await page.getByRole('tab', { name: 'Activity', exact: true }).click();
    await expect(row).toBeVisible({ timeout: 1000 });
  }).toPass({ timeout: 20_000 });
  await expect(row).toContainText('Key');
  await expect(row).toContainText('whoami');
  await expect(row).toContainText('OK');
  await expect(row).toContainText(/\d+ ms/);

  // Filtered to the key, and to refusals, of which it has none.
  await page.getByLabel('Connection').selectOption({ label: 'Nightly report script' });
  await expect(row).toBeVisible();
  await page.getByRole('button', { name: 'Refused', exact: true }).click();
  await expect(page.getByText('No matching calls')).toBeVisible();
  await page.getByRole('button', { name: 'All', exact: true }).click();
  await expect(row).toBeVisible();

  // A key past 10,000 calls a UTC day is noted once, across the table rather than as a call.
  const capped = new Date().toISOString();
  dispatch.database('data/platform/accounts.sqlite', (db) =>
    db
      .prepare(
        `INSERT INTO agent_activity(at,key_id,key_name,key_kind,surface,outcome,ms,bytes)
         VALUES (?,?,?,'key','activity:capped','capped',0,0)`,
      )
      .run(Date.parse(capped), made.value.key.id, 'Nightly report script'),
  );
  await page.getByRole('tab', { name: 'Keys', exact: true }).click();
  await page.getByRole('tab', { name: 'Activity', exact: true }).click();
  const note = page.getByRole('row').filter({ hasText: 'Over 10,000 calls' });
  await expect(note.getByRole('cell')).toHaveCount(1);
  await expect(note).toHaveText(
    `Over 10,000 calls from Nightly report script on ${utcDay(capped)} (UTC); later calls that day aren’t listed.`,
  );
  await expect(page.getByRole('row')).toHaveCount(3);
});

test('the owner changes where a connected app reaches, without being asked to sign in again', async ({
  page,
  baseURL,
  dispatch,
}) => {
  await login(page);
  await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();
  const owner = await dispatch.client();
  const pairing = await owner.post('/api/platform/oauth/pairing');
  expect(pairing.status, pairing.body).toBe(200);
  await connectApp(page, baseURL!, 'Laptop – Claude Code');
  const starting = await startingTools(dispatch);
  const agents = async () => (await owner.get('/api/platform/agents')).value as AgentKeys;
  const { dsps } = await agents();
  const summit = dsps.find((dsp) => dsp.name === 'Summit Delivery')!;
  const north = dsps.find((dsp) => dsp.name === 'Northline Logistics')!;
  // Long after signing in, removing a DSP asks for the password again; where an app reaches
  // changes without it.
  dispatch.database('data/platform/accounts.sqlite', (db) => {
    db.prepare('UPDATE sessions SET created_at=0').run();
    db.prepare('UPDATE session_security SET verified_at=0, password_verified_at=0').run();
  });
  const remove = await owner.post(`/api/platform/dsps/${summit.id}/remove`, {});
  expect([remove.status, remove.value.error], remove.body).toEqual([403, 'sign_in_again']);

  await page.goto(`/${platformHash('agents')}`);
  const row = page.getByRole('row').filter({ hasText: 'Laptop – Claude Code' });
  await expect(row).toContainText('All DSPs');
  await expect(row).toContainText(starting.text);
  await row.getByRole('button', { name: 'Edit Laptop – Claude Code', exact: true }).click();
  const sheet = page.getByRole('dialog', { name: 'Laptop – Claude Code' });
  await expect(sheet.getByText('Known metadata', { exact: true })).toBeVisible();
  // An app only ever reads, and never expires.
  await expect(sheet.getByRole('radio', { name: /Operator/ })).toHaveCount(0);
  await expect(sheet.getByLabel('Expires')).toHaveCount(0);
  await sheet.getByText('Choose DSPs', { exact: true }).click();
  await sheet.getByRole('checkbox', { name: 'Northline Logistics' }).check();

  // Saved at once: no password asked.
  await sheet.getByRole('button', { name: 'Save changes', exact: true }).click();
  await expect(page.getByText('App updated', { exact: true })).toBeVisible();
  await expect(page.getByRole('dialog', { name: 'Confirm it’s you' })).toHaveCount(0);
  await expect(sheet).toHaveCount(0);
  await expect(row).toContainText('1 DSP');
  await expect(row).toContainText('Northline Logistics');
  const saved = (await agents()).keys.find((key) => key.name === 'Laptop – Claude Code')!;
  expect([saved.allDsps, saved.dsps]).toEqual([false, [north.id]]);
});
