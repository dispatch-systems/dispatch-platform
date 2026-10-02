import fs from 'node:fs';
import type { Locator } from '@playwright/test';
import { signIns } from '../../dashboard/src/lib/agents.js';
import { timeOfDay } from '../../dashboard/src/lib/format.js';
import { test, expect, login } from './fixtures.js';

test('the platform owner makes a key, sees it once, tests it, changes and revokes it', async ({
  page,
}) => {
  await login(page);
  await page.getByRole('link', { name: 'Agents', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Agents', level: 1 })).toBeVisible();
  await expect(page.getByText('No keys yet')).toBeVisible();

  await page.getByRole('button', { name: 'New key', exact: true }).click();
  const sheet = page.getByRole('dialog', { name: 'New key' });
  await sheet.getByLabel('Name').fill('Laptop – Claude Code');
  await sheet.getByText('Choose DSPs', { exact: true }).click();
  await sheet.getByRole('checkbox', { name: 'Northline Logistics' }).check();
  await expect(sheet.getByRole('radio', { name: /Read only/ })).toBeChecked();
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
  await expect(ready.getByRole('status')).toContainText('Connected · Read only · 1 DSP');
  await ready.getByRole('button', { name: 'Done', exact: true }).click();

  const row = page.getByRole('row').filter({ hasText: 'Laptop – Claude Code' });
  await expect(row).toContainText(`…${token!.slice(-4)}`);
  await expect(row).toContainText('Read only');
  await expect(row).toContainText('Northline Logistics');
  // The page never shows the key again.
  await expect(page.getByText(token!)).toHaveCount(0);

  // A changed key asks before its edits are thrown away.
  await row.getByRole('button', { name: 'Open Laptop – Claude Code' }).click();
  const edit = page.getByRole('dialog', { name: 'Laptop – Claude Code' });
  await edit.getByText('Operator', { exact: true }).click();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Save changes', exact: true }).last().click();
  await expect(row).toContainText('Operator');

  // Testing a key from the Connect tab answers as an agent would.
  await page.getByRole('tab', { name: 'Connect', exact: true }).click();
  const connect = page.getByRole('region', { name: 'Keys', exact: true });
  await connect.getByLabel('Key to test').fill(token!);
  await connect.getByRole('button', { name: 'Test key', exact: true }).click();
  await expect(connect.getByRole('status')).toContainText('Connected · Operator · 1 DSP');
  // The addresses an agent needs, the OpenAPI spec and the skill.
  await expect(connect.locator('.agents-endpoint').first()).toContainText('/api/v1/mcp');
  await expect(connect.getByRole('link', { name: 'OpenAPI spec' })).toHaveAttribute(
    'href',
    '/api/platform/agents/openapi.json',
  );
  const download = page.waitForEvent('download');
  await connect.getByRole('link', { name: 'Download Dispatch skill' }).click();
  const skill = await download;
  expect(skill.suggestedFilename()).toBe('SKILL.md');
  const text = fs.readFileSync((await skill.path())!, 'utf8');
  expect(text).toMatch(/^---\nname: dispatch\n/);

  // Revoked, it stops at once.
  await page.getByRole('tab', { name: 'Keys', exact: true }).click();
  await row.getByRole('button', { name: 'Open Laptop – Claude Code' }).click();
  await page.getByRole('button', { name: 'Revoke key', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'Revoke Laptop – Claude Code?' })
    .getByRole('button', { name: 'Revoke key', exact: true })
    .click();
  await expect(page.getByRole('button', { name: 'Show 1 revoked or expired key' })).toBeVisible();
  await page.getByRole('tab', { name: 'Connect', exact: true }).click();
  await connect.getByLabel('Key to test').fill(token!);
  await connect.getByRole('button', { name: 'Test key', exact: true }).click();
  await expect(connect.getByRole('status')).toContainText('That key was revoked.');
});

test('the Connect tab signs each app in with one command or a few steps, ready to copy, and copying lets apps connect for ten minutes', async ({
  page,
  baseURL,
}) => {
  const mcp = `${baseURL}/api/v1/mcp`;
  await login(page);
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write'], { origin: baseURL });
  await page.getByRole('link', { name: 'Agents', exact: true }).click();
  await page.getByRole('tab', { name: 'Connect', exact: true }).click();
  const signIn = page.getByRole('region', { name: 'Sign in', exact: true });
  await expect(signIn).toContainText('Recommended');
  const clipboard = () => page.evaluate(() => navigator.clipboard.readText());
  // Until the owner copies a way to sign in, or lets them, no app may start connecting.
  const status = signIn.locator('.agents-pairing [aria-live]');
  const openUntil = async (): Promise<string | null> =>
    (await page.evaluate(() => fetch('/api/platform/oauth/pairing').then((r) => r.json())))
      .openUntil;
  await expect(status).toHaveText('');
  expect(await openUntil()).toBeNull();
  // Each copy, and the button, lets apps connect for the next ten minutes.
  const opens = async (button: Locator) => {
    const opened = page.waitForResponse(
      (response) =>
        response.request().method() === 'POST' &&
        new URL(response.url()).pathname === '/api/platform/oauth/pairing',
    );
    await button.click();
    expect((await opened).status()).toBe(200);
  };
  const copy = async (panel: ReturnType<typeof signIn.getByRole>, name: string) => {
    await opens(panel.getByRole('button', { name, exact: true }));
    await expect(panel.getByRole('button', { name, exact: true })).toHaveText('Copied');
    return clipboard();
  };

  // Each terminal app: its one command, what happens next, and a prompt that asks it to run it.
  const commands = [
    [
      'Claude Code',
      `claude mcp add --transport http --scope user dispatch ${mcp} && claude mcp login dispatch`,
    ],
    ['Codex', `codex mcp add dispatch --url ${mcp}`],
    ['Hermes', `hermes mcp add dispatch --url ${mcp} --auth oauth --connect-timeout 300`],
  ] as const;
  const prompts = signIns(baseURL!).terminals;
  for (const [index, [app, command]] of commands.entries()) {
    await signIn.getByRole('tab', { name: app, exact: true }).click();
    const panel = signIn.getByRole('tabpanel', { name: app, exact: true });
    await expect(panel.locator('code').first()).toHaveText(command);
    await expect(panel).toContainText('A Dispatch page opens — approve it there');
    expect(await copy(panel, `Copy ${app} command`)).toBe(command);
    if (index === 0) {
      // The first copy opened connecting; the tab says until when.
      await expect(status).toHaveText(/^Connecting is open until \d{1,2}:\d{2} [AP]M$/);
      const until = await openUntil();
      expect(Date.parse(until!) - Date.now()).toBeGreaterThan(9 * 60_000);
      expect(Date.parse(until!) - Date.now()).toBeLessThanOrEqual(10 * 60_000);
      await expect(status).toHaveText(
        `Connecting is open until ${timeOfDay(until!, 'America/Chicago')}`,
      );
    }
    const prompt = await copy(panel, `Copy prompt for ${app}`);
    expect(prompt).toBe(prompts[index]!.prompt);
    expect(prompt).toContain(mcp);
  }
  // From a machine with no browser: the link is opened anywhere and the address pasted back.
  await signIn.getByRole('tab', { name: 'Codex', exact: true }).click();
  const codex = signIn.getByRole('tabpanel', { name: 'Codex', exact: true });
  await codex.getByText('On another machine?').click();
  expect(await copy(codex, 'Copy Codex remote command')).toBe(
    'codex mcp login dispatch --no-browser',
  );
  await expect(codex).toContainText('paste it into the terminal');

  // ChatGPT: a few steps, with the address to copy.
  await signIn.getByRole('tab', { name: 'ChatGPT', exact: true }).click();
  const chatgpt = signIn.getByRole('tabpanel', { name: 'ChatGPT', exact: true });
  await expect(chatgpt).toContainText('Create custom MCP server');
  await expect(chatgpt).toContainText('Authentication: OAuth');
  await expect(chatgpt).toContainText('Needs ChatGPT Plus, Pro, Business or Enterprise.');
  expect(await copy(chatgpt, 'Copy MCP address')).toBe(mcp);

  // Any other app: the address, a JSON entry, and a local bridge.
  await signIn.getByRole('tab', { name: 'Other apps', exact: true }).click();
  const other = signIn.getByRole('tabpanel', { name: 'Other apps', exact: true });
  expect(await copy(other, 'Copy MCP address')).toBe(mcp);
  expect(JSON.parse(await copy(other, 'Copy JSON config'))).toEqual({
    mcpServers: { dispatch: { url: mcp } },
  });
  expect(await copy(other, 'Copy local server command')).toBe(`npx -y mcp-remote ${mcp}`);
  await expect(other).toContainText('use a key below');

  // The button lets apps connect too, extending the window.
  const before = Date.parse((await openUntil())!);
  await opens(signIn.getByRole('button', { name: 'Allow connecting for 10 minutes', exact: true }));
  expect(Date.parse((await openUntil())!)).toBeGreaterThanOrEqual(before);
  await expect(status).toHaveText(/^Connecting is open until \d{1,2}:\d{2} [AP]M$/);
});

test('the owner chooses which apps may connect', async ({ page }) => {
  await login(page);
  await page.getByRole('link', { name: 'Agents', exact: true }).click();
  await page.getByRole('tab', { name: 'Connect', exact: true }).click();
  const allowed = page.getByRole('region', { name: 'Apps that may connect', exact: true });
  // The four known apps and apps on this computer may; websites and other apps may not.
  const apps = [
    ['ChatGPT', true],
    ['Codex', true],
    ['Claude Code', true],
    ['Hermes Agent', true],
    ['Apps on this computer', true],
    ['Websites and other apps', false],
  ] as const;
  await expect(allowed.getByRole('switch')).toHaveCount(apps.length);
  for (const [name, on] of apps)
    await expect(allowed.getByRole('switch', { name, exact: true })).toBeChecked({ checked: on });

  // Turned off, an app stays off until it is turned back on.
  const codex = allowed.getByRole('switch', { name: 'Codex', exact: true });
  await codex.click();
  await expect(codex).not.toBeChecked();
  await expect(page.getByText('Codex may no longer connect')).toBeVisible();
  await page.reload();
  await expect(codex).not.toBeChecked();
  await expect(allowed.getByRole('switch', { name: 'ChatGPT', exact: true })).toBeChecked();
  await codex.click();
  await expect(codex).toBeChecked();
  await expect(page.getByText('Codex may connect')).toBeVisible();
  await page.reload();
  await expect(codex).toBeChecked();
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
    tools: 'full',
    locations: false,
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

  // A key past 10,000 calls a day is noted once, across the table rather than as a call.
  dispatch.database('data/platform/accounts.sqlite', (db) =>
    db
      .prepare(
        `INSERT INTO agent_activity(at,key_id,key_name,key_kind,surface,outcome,ms,bytes)
         VALUES (?,?,?,'key','activity:capped','capped',0,0)`,
      )
      .run(Date.now(), made.value.key.id, 'Nightly report script'),
  );
  await page.getByRole('tab', { name: 'Keys', exact: true }).click();
  await page.getByRole('tab', { name: 'Activity', exact: true }).click();
  const note = page.getByRole('row').filter({ hasText: 'Over 10,000 calls' });
  await expect(note.getByRole('cell')).toHaveCount(1);
  await expect(note).toHaveText(
    'Over 10,000 calls today from Nightly report script; later calls today aren’t listed.',
  );
  await expect(page.getByRole('row')).toHaveCount(3);
});
