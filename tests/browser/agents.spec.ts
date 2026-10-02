import fs from 'node:fs';
import { signIns } from '../../dashboard/src/lib/agents.js';
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

test('the Connect tab signs each app in with one command or a few steps, ready to copy', async ({
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
  const copy = async (panel: ReturnType<typeof signIn.getByRole>, name: string) => {
    await panel.getByRole('button', { name, exact: true }).click();
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
});
