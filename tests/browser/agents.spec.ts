import fs from 'node:fs';
import { signIns } from '../../dashboard/src/lib/agents.js';
import { utcDay } from '../../dashboard/src/lib/format.js';
import { test, expect, login } from './fixtures.js';

test('the platform owner makes a key, sees it once, tests it, changes and revokes it', async ({
  page,
  baseURL,
}) => {
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

  // Using keys, folded away beneath them: testing a key answers as an agent would.
  const using = page.getByRole('group').filter({ hasText: 'Using keys' });
  await expect(using.locator('summary')).toHaveText(
    'Using keys· addresses, key tester, OpenAPI spec and skill',
  );
  await expect(using.getByLabel('Key to test')).toBeHidden();
  await using.locator('summary').click();
  await expect(using.locator('summary')).toHaveText('Using keys');
  await using.getByLabel('Key to test').fill(token!);
  await using.getByRole('button', { name: 'Test key', exact: true }).click();
  await expect(using.getByRole('status')).toContainText('Connected · Operator · 1 DSP');
  // The addresses an agent needs, ready to copy, the OpenAPI spec and the skill.
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
  await expect(using.getByRole('link', { name: 'OpenAPI spec' })).toHaveAttribute(
    'href',
    '/api/platform/agents/openapi.json',
  );
  const download = page.waitForEvent('download');
  await using.getByRole('link', { name: 'Download Dispatch skill' }).click();
  const skill = await download;
  expect(skill.suggestedFilename()).toBe('SKILL.md');
  const text = fs.readFileSync((await skill.path())!, 'utf8');
  expect(text).toMatch(/^---\nname: dispatch\n/);

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

test('Connect an app shows each app’s steps, ready to copy, and copying lets apps connect for ten minutes', async ({
  page,
  baseURL,
}) => {
  const mcp = `${baseURL}/api/v1/mcp`;
  // The browser's own clock, moved on at the end to when connecting closes.
  await page.clock.install();
  await login(page);
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write'], { origin: baseURL });
  await page.getByRole('link', { name: 'Agents', exact: true }).click();
  // With no app connected yet, Apps offers to connect one.
  await expect(page.getByRole('heading', { name: 'No apps connected yet' })).toBeVisible();
  await expect(
    page.getByText('Use Dispatch from ChatGPT, Claude Code and other AI apps.'),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Connect an app', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toHaveAccessibleName('Connect an app');
  const tiles = dialog.getByRole('group', { name: 'Which app?' }).getByRole('button');
  await expect(tiles).toHaveText(['ChatGPT', 'Codex CLI', 'Claude Code', 'Hermes', 'Other app']);
  const pick = async (app: string, title: string) => {
    await tiles.getByText(app, { exact: true }).click();
    await expect(dialog).toHaveAccessibleName(title);
  };
  const back = async () => {
    await dialog.getByRole('button', { name: 'Back', exact: true }).click();
    await expect(dialog).toHaveAccessibleName('Connect an app');
  };

  const status = dialog.locator('.agents-connect-status');
  const waiting = (who: string) =>
    new RegExp(`^Waiting for ${who} to connect…\\((10:00|9:[0-5]\\d) left\\)$`);
  const openUntil = async (): Promise<string | null> =>
    (await page.evaluate(() => fetch('/api/platform/oauth/pairing').then((r) => r.json())))
      .openUntil;
  // Each copy lets apps connect for the next ten minutes, and copies exactly what it shows.
  const copy = async (name: string) => {
    const opened = page.waitForResponse(
      (response) =>
        response.request().method() === 'POST' &&
        new URL(response.url()).pathname === '/api/platform/oauth/pairing',
    );
    await dialog.getByRole('button', { name, exact: true }).click();
    expect((await opened).status()).toBe(200);
    await expect(dialog.getByRole('button', { name, exact: true })).toHaveText('Copied');
    return page.evaluate(() => navigator.clipboard.readText());
  };

  // Each terminal app: its one command, the way from another computer, and a prompt that asks
  // it to set itself up.
  const terminals = signIns(baseURL!).terminals;
  const apps = [
    ['Codex CLI', `codex mcp add dispatch --url ${mcp}`, 'codex mcp login dispatch --no-browser'],
    [
      'Claude Code',
      `claude mcp add --transport http --scope user dispatch ${mcp} && claude mcp login dispatch`,
      `claude mcp add --transport http --scope user dispatch ${mcp} && claude mcp login dispatch --no-browser`,
    ],
    ['Hermes', `hermes mcp add dispatch --url ${mcp} --auth oauth --connect-timeout 300`, null],
  ] as const;
  for (const [index, [app, command, remote]] of apps.entries()) {
    await pick(app, `Connect ${app}`);
    const steps = dialog.getByRole('list').first().getByRole('listitem');
    await expect(steps).toHaveText([
      `1Copy this command${command}Copy`,
      '2Run it in your terminal',
      app === 'Hermes'
        ? '3Approve it on the Dispatch page that opens, then answer Y to turn on the tools'
        : '3Approve it on the Dispatch page that opens',
    ]);
    // Opening an app's steps lets no app connect: only copying one does.
    await expect(status).toHaveText('Copy the command to start.');
    if (index === 0) expect(await openUntil()).toBeNull();
    expect(await copy(`Copy ${app} command`)).toBe(command);
    await expect(status).toHaveText(waiting(app));
    if (index === 0) {
      const until = await openUntil();
      expect(Date.parse(until!) - Date.now()).toBeGreaterThan(9 * 60_000);
      expect(Date.parse(until!) - Date.now()).toBeLessThanOrEqual(10 * 60_000);
    }
    await dialog.getByText('Using another computer?', { exact: true }).click();
    if (remote) expect(await copy(`Copy ${app} remote command`)).toBe(remote);
    await expect(dialog).toContainText('paste it into the terminal');
    await dialog.getByText(`Let ${app} set itself up instead (copy a prompt)`).click();
    const prompt = terminals.find((terminal) => terminal.label === app)!.prompt;
    await expect(dialog.locator('.agents-prompt')).toHaveText(prompt);
    expect(await copy(`Copy prompt for ${app}`)).toBe(prompt);
    await back();
  }

  // ChatGPT: where to click, and the form's fields with the address to copy.
  await pick('ChatGPT', 'Connect ChatGPT');
  await expect(dialog.getByRole('list').first().getByRole('listitem')).toHaveText([
    '1On chatgpt.com, open Plugins then Add then Create custom MCP serverNeeds ChatGPT Plus or higher.',
    `2Fill in the form, then click CreateNameDispatchURL${mcp}CopyAuthenticationOAuth`,
    '3Approve it on the Dispatch page that opens',
  ]);
  await expect(dialog.getByRole('link', { name: 'chatgpt.com', exact: true })).toHaveAttribute(
    'href',
    'https://chatgpt.com',
  );
  await expect(status).toHaveText('Copy the URL to start.');
  expect(await copy('Copy MCP address')).toBe(mcp);
  await expect(status).toHaveText(waiting('ChatGPT'));
  await back();

  // Any other app: the address, a local bridge, and a key for an app that can't sign in.
  await pick('Other app', 'Connect another app');
  await expect(dialog.getByRole('list').first().getByRole('listitem')).toHaveText([
    `1Add Dispatch as an MCP server in your app with this address${mcp}Copy`,
    '2Your app opens Dispatch to sign in — approve it there',
  ]);
  await expect(status).toHaveText('Copy the address to start.');
  expect(await copy('Copy MCP address')).toBe(mcp);
  await expect(status).toHaveText(waiting('your app'));
  await expect(dialog).toContainText(`Only runs local servers? Use npx -y mcp-remote ${mcp}`);
  expect(await copy('Copy local server command')).toBe(`npx -y mcp-remote ${mcp}`);

  // Ten minutes on, connecting has closed, and the dialog says to copy again.
  await page.clock.fastForward('10:01');
  await expect(status).toHaveText('Time ran out. Copy the address again.');
  await expect(status.getByRole('button')).toHaveCount(0);

  await dialog.getByRole('link', { name: 'Use a key instead', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('tab', { name: 'Keys', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await expect(page.getByText('No keys yet')).toBeVisible();
});

test('the owner chooses which apps may connect', async ({ page }) => {
  await login(page);
  await page.getByRole('link', { name: 'Agents', exact: true }).click();
  const choose = page.getByRole('button', { name: 'Choose which apps may connect', exact: true });
  await choose.click();
  const allowed = page.getByRole('dialog', { name: 'Apps that may connect', exact: true });
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
  // The kinds that aren't one app say what they are.
  await expect(
    allowed.getByRole('switch', { name: 'Apps on this computer', exact: true }),
  ).toHaveAccessibleDescription('Other apps that sign in from your computer');
  await expect(
    allowed.getByRole('switch', { name: 'Websites and other apps', exact: true }),
  ).toHaveAccessibleDescription('Apps that sign in from a website');
  await expect(allowed).toContainText(
    'Turning an app off stops new connections. Apps already connected keep working until you revoke them.',
  );

  // Turned off, an app stays off until it is turned back on.
  const codex = allowed.getByRole('switch', { name: 'Codex', exact: true });
  await codex.click();
  await expect(codex).not.toBeChecked();
  await expect(page.getByText('Codex may no longer connect')).toBeVisible();
  await page.reload();
  await choose.click();
  await expect(codex).not.toBeChecked();
  await expect(allowed.getByRole('switch', { name: 'ChatGPT', exact: true })).toBeChecked();
  await codex.click();
  await expect(codex).toBeChecked();
  await expect(page.getByText('Codex may connect')).toBeVisible();
  await page.reload();
  await choose.click();
  await expect(codex).toBeChecked();
  await allowed.getByRole('button', { name: 'Close dialog', exact: true }).click();
  await expect(allowed).toHaveCount(0);
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
