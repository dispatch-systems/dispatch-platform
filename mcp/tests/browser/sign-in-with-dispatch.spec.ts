import crypto from 'node:crypto';
import { platformHash } from '../../../core/shell/frontend/runtime/navigation.js';
import { test, expect, login } from '../../../core/shell/tests/support/fixtures.js';

const claudeCode = 'https://claude.ai/oauth/claude-code-client-metadata';
// Claude Code's own listener on this computer; the test answers for it.
const callback = 'http://localhost:43821/callback';

test('Dispatch turns away an app it does not know, without sending the browser to it', async ({
  page,
}) => {
  await login(page);
  await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();
  const query = new URLSearchParams({
    response_type: 'code',
    client_id: 'https://apps.example.test/client.json',
    redirect_uri: callback,
    code_challenge: crypto.createHash('sha256').update('verifier').digest('base64url'),
    code_challenge_method: 'S256',
  });
  await page.goto(`/oauth/authorize?${query}`);
  await expect(page).toHaveURL(/#authorize\?error=/);
  await expect(page.getByText('Dispatch doesn’t accept this app.')).toBeVisible();
});

test('an app connects only while the owner lets apps connect and it is on, and only in its own browser; a website says where access goes', async ({
  page,
  request,
  dispatch,
  baseURL,
}) => {
  const query = new URLSearchParams({
    response_type: 'code',
    client_id: claudeCode,
    redirect_uri: callback,
    code_challenge: crypto.createHash('sha256').update('verifier').digest('base64url'),
    code_challenge_method: 'S256',
    state: 'closed',
    resource: `${baseURL}/api/v1/mcp`,
  });
  await login(page);
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write'], { origin: baseURL });
  await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();

  // Closed, the request is turned away. A link can't open connecting: only Dispatch's own
  // Connect an app does, and then the owner starts again from the app.
  await page.goto(`/oauth/authorize?${query}`);
  await expect(page).toHaveURL(/#authorize\?error=pairing_closed/);
  await expect(
    page.getByText(
      'Connecting is closed. Open Agents → Apps → Connect an app, copy your app’s command, ' +
        'then start again from your app.',
      { exact: true },
    ),
  ).toBeVisible();
  await expect(page.locator('.agents-authorize').getByRole('button')).toHaveCount(0);
  const apps = page.getByRole('link', { name: 'Agents → Apps', exact: true });
  await expect(apps).toHaveAttribute('href', platformHash('agents', { tab: 'apps' }));
  await apps.click();
  await expect(page.getByRole('tab', { name: 'Apps', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await page.getByRole('button', { name: 'Connect an app', exact: true }).click();
  const connecting = page.getByRole('dialog');
  await connecting.getByRole('button', { name: 'Claude Code', exact: true }).click();
  await connecting.getByRole('button', { name: 'Copy Claude Code command', exact: true }).click();
  await expect(connecting.locator('.agents-connect-status')).toHaveText(
    /^Waiting for Claude Code to connect…\((10:00|9:[0-5]\d) left\)$/,
  );
  await page.goto(`/oauth/authorize?${query}`);
  await expect(page).toHaveURL(/#authorize\?request=/);
  await expect(page.getByRole('form', { name: 'Claude Code' })).toBeVisible();

  // An app the owner turned off is turned away by name, with the way to turn it back on.
  const owner = await dispatch.client();
  const off = await owner.post('/api/platform/oauth/apps', { id: 'claude-code', allowed: false });
  expect(off.status, off.body).toBe(200);
  await page.goto(`/oauth/authorize?${query}`);
  await expect(page).toHaveURL(/#authorize\?error=app_not_allowed&app=claude-code/);
  await expect(
    page.getByText(
      'Dispatch doesn’t accept Claude Code yet. Turn it on under Agents → Apps → Choose which ' +
        'apps may connect.',
      { exact: true },
    ),
  ).toBeVisible();
  await page.getByRole('link', { name: 'Agents → Apps', exact: true }).click();
  await expect(page.getByRole('tab', { name: 'Apps', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await page.getByRole('button', { name: 'Choose which apps may connect', exact: true }).click();
  const claude = page
    .getByRole('dialog', { name: 'Apps that may connect', exact: true })
    .getByRole('switch', { name: 'Claude Code', exact: true });
  await expect(claude).not.toBeChecked();
  await claude.click();
  await expect(claude).toBeChecked();
  await page.goto(`/oauth/authorize?${query}`);
  await expect(page).toHaveURL(/#authorize\?request=/);
  await expect(page.getByRole('form', { name: 'Claude Code' })).toBeVisible();
  // A kind of app the page doesn't know is this app.
  await page.goto(platformHash('authorize', { error: 'app_not_allowed', app: 'cursor' }));
  await expect(
    page.getByText(
      'Dispatch doesn’t accept this app yet. Turn it on under Agents → Apps → Choose which apps ' +
        'may connect.',
      { exact: true },
    ),
  ).toBeVisible();

  // A request is answered only in the browser that started it: its link, opened in another,
  // asks nothing there.
  const started = await request.get(`/oauth/authorize?${query}`, { maxRedirects: 0 });
  expect(started.status()).toBe(302);
  const elsewhere = started.headers()['location']!;
  expect(elsewhere).toMatch(/#authorize\?request=/);
  await page.goto(elsewhere);
  await expect(
    page.getByText(
      /^This approval isn’t open in this browser, or it has expired\. Start connecting again from your app, and approve it in the browser that opens\./,
    ),
  ).toBeVisible();
  await expect(page.getByRole('link', { name: 'Back to Agents', exact: true })).toHaveAttribute(
    'href',
    platformHash('agents'),
  );
  await expect(page.getByRole('form')).toHaveCount(0);
  await expect(page.locator('.agents-authorize').getByRole('button')).toHaveCount(0);

  // A website, once websites may connect, shows first where it sends access.
  const web = await owner.post('/api/platform/oauth/apps', { id: 'web', allowed: true });
  expect(web.status, web.body).toBe(200);
  const site = 'https://app.example.com/callback';
  const registered = await dispatch.request('/oauth/register', {
    client_name: 'Route Planner',
    redirect_uris: [site],
    token_endpoint_auth_method: 'none',
    grant_types: ['authorization_code', 'refresh_token'],
    response_types: ['code'],
  });
  expect(registered.status, registered.body).toBe(201);
  query.set('client_id', registered.value.client_id);
  query.set('redirect_uri', site);
  await page.goto(`/oauth/authorize?${query}`);
  const website = page.getByRole('form', { name: 'Route Planner' });
  await expect(website.getByText('App-provided metadata', { exact: true })).toBeVisible();
  await expect(website).toContainText('Unrecognized app: it says it is “Route Planner”.');
  await expect(website.locator('.agents-sends')).toHaveText('Sends access to: app.example.com');
});
