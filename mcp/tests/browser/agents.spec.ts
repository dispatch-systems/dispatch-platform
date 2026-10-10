import { signIns } from '../../frontend/agents.js';
import { test, expect, login } from '../../../core/shell/tests/support/fixtures.js';

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
  // Only the title and its button: no description under it.
  await expect(page.locator('.empty p')).toHaveCount(0);
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
    '1On chatgpt.com, open Plugins then Add then Create custom MCP serverAvailability depends on your current ChatGPT plan and workspace settings. Custom MCP apps are currently used on the web. Check OpenAI’s current availability.',
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
  await expect(
    page.getByText('Codex may no longer make new connections; existing connections remain active'),
  ).toBeVisible();
  await page.reload();
  await choose.click();
  await expect(codex).not.toBeChecked();
  await expect(allowed.getByRole('switch', { name: 'ChatGPT', exact: true })).toBeChecked();
  await codex.click();
  await expect(codex).toBeChecked();
  await expect(page.getByText('Codex may make new connections')).toBeVisible();
  await page.reload();
  await choose.click();
  await expect(codex).toBeChecked();
  await allowed.getByRole('button', { name: 'Close dialog', exact: true }).click();
  await expect(allowed).toHaveCount(0);
});
