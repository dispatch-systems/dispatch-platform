import crypto from 'node:crypto';
import { test, expect, login, signIn } from './fixtures.js';

const claudeCode = 'https://claude.ai/oauth/claude-code-client-metadata';
// Claude Code's own listener on this computer; the test answers for it.
const callback = 'http://localhost:43821/callback';

test('an app signs in with Dispatch: the owner signs in, approves it, then revokes it', async ({
  page,
  request,
  baseURL,
  dispatch,
}) => {
  const verifier = crypto.randomBytes(32).toString('base64url');
  const state = crypto.randomBytes(16).toString('base64url');
  const resource = `${baseURL}/api/v1/mcp`;
  const query = (state: string) =>
    new URLSearchParams({
      response_type: 'code',
      client_id: claudeCode,
      redirect_uri: callback,
      code_challenge: crypto.createHash('sha256').update(verifier).digest('base64url'),
      code_challenge_method: 'S256',
      state,
      resource,
    });
  await page.route('http://localhost:43821/**', (route) =>
    route.fulfill({ contentType: 'text/html', body: '<p>Authentication complete.</p>' }),
  );

  // The owner lets apps start connecting, as copying a way to sign in from the Connect tab does.
  const owner = await dispatch.client();
  expect((await owner.post('/api/platform/oauth/pairing')).status).toBe(200);

  // Signed out, the request waits through sign-in on the approval page.
  await page.goto(`/oauth/authorize?${query(state)}`);
  await expect(page).toHaveURL(/#authorize\?request=/);
  const approvalUrl = page.url();
  await signIn(page);
  await expect(page.getByRole('heading', { name: 'Connect an app', level: 1 })).toBeVisible();
  expect(page.url()).toBe(approvalUrl);

  const approval = page.getByRole('form', { name: 'Claude Code' });
  await expect(approval.getByText('Verified', { exact: true })).toBeVisible();
  await expect(approval.getByText('this computer', { exact: true })).toBeVisible();
  // Every request asks the owner to approve only what they started.
  await expect(
    approval.getByText(
      'Only approve if you started connecting Claude Code yourself just now. If you didn’t, ' +
        'choose Deny. Access will be sent to an app running on this computer.',
    ),
  ).toBeVisible();
  await expect(approval.getByText(/^Approving replaces/)).toHaveCount(0);
  const name = approval.getByLabel('Connection name');
  await expect(name).toHaveValue('Claude Code');
  await name.fill('Laptop – Claude Code app');
  await approval.getByText('Choose DSPs', { exact: true }).click();
  await approval.getByRole('checkbox', { name: 'Northline Logistics' }).check();
  await approval.getByRole('button', { name: 'Essential', exact: true }).click();
  await approval.getByRole('button', { name: 'Approve', exact: true }).click();

  // The browser goes back to the app with a code for it, its state and the issuer.
  await page.waitForURL((url) => url.href.startsWith(`${callback}?`));
  const answer = new URL(page.url());
  expect(answer.searchParams.get('state')).toBe(state);
  expect(answer.searchParams.get('iss')).toBe(baseURL);
  const code = answer.searchParams.get('code');
  expect(code).toBeTruthy();

  // The app redeems the code, and its token reads Dispatch.
  const token = await request.post('/oauth/token', {
    form: {
      grant_type: 'authorization_code',
      code: code!,
      redirect_uri: callback,
      code_verifier: verifier,
      client_id: claudeCode,
      resource,
    },
  });
  expect(token.status(), await token.text()).toBe(200);
  const bearer = { Authorization: `Bearer ${(await token.json()).access_token}` };
  expect((await request.get('/api/v1/whoami', { headers: bearer })).status()).toBe(200);

  // An answered request can't be answered again.
  await page.goto(approvalUrl);
  await expect(
    page.getByText('This request expired. Start the connection again from your app.'),
  ).toBeVisible();

  // Connecting it again under that name says which connection it replaces; the owner denies.
  await page.goto(`/oauth/authorize?${query('again')}`);
  const again = page.getByRole('form', { name: 'Claude Code' });
  await expect(again.getByText(/^Approving replaces/)).toHaveCount(0);
  await again.getByLabel('Connection name').fill('Laptop – Claude Code app');
  await expect(again.getByText(/^Approving replaces/)).toHaveText(
    /^Approving replaces “Laptop – Claude Code app”, connected \w{3} \d{1,2}, \d{4}\. Its current connection stops working\.$/,
  );
  await again.getByRole('button', { name: 'Deny', exact: true }).click();
  await page.waitForURL((url) => url.href.startsWith(`${callback}?`));
  const denied = new URL(page.url()).searchParams;
  expect([denied.get('error'), denied.get('state'), denied.get('iss')]).toEqual([
    'access_denied',
    'again',
    baseURL,
  ]);

  // The app is a connected app, not a key, and revoking it ends its access at once.
  await page.goto('/');
  await page.getByRole('link', { name: 'Agents', exact: true }).click();
  await expect(page.getByText('No keys yet')).toBeVisible();
  await page.getByRole('tab', { name: 'Connected apps', exact: true }).click();
  const row = page.getByRole('row').filter({ hasText: 'Laptop – Claude Code app' });
  await expect(row).toContainText('Claude Code');
  await expect(row).toContainText('Verified');
  await expect(row).toContainText('Northline Logistics');
  await expect(row).toContainText('Essential tools');
  await row.getByRole('button', { name: 'Revoke Laptop – Claude Code app', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'Revoke Laptop – Claude Code app?' })
    .getByRole('button', { name: 'Revoke app', exact: true })
    .click();
  await expect(page.getByRole('button', { name: 'Show 1 revoked or expired app' })).toBeVisible();
  expect((await request.get('/api/v1/whoami', { headers: bearer })).status()).toBe(401);
});

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

test('an app connects only while the owner lets apps connect and it is on; a website says where access goes', async ({
  page,
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
  await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();

  // Closed, the request is turned away; the owner opens connecting, then starts again.
  await page.goto(`/oauth/authorize?${query}`);
  await expect(page).toHaveURL(/#authorize\?error=pairing_closed/);
  const closed = page.getByText('Connecting is closed.', { exact: true });
  await expect(closed).toBeVisible();
  await page.getByRole('button', { name: 'Allow connecting for 10 minutes', exact: true }).click();
  await expect(closed).toHaveCount(0);
  await expect(
    page.getByText(
      /^Connecting is open until \d{1,2}:\d{2} [AP]M\. Now start again from your app\.$/,
    ),
  ).toBeVisible();
  await page.goto(`/oauth/authorize?${query}`);
  await expect(page).toHaveURL(/#authorize\?request=/);
  await expect(page.getByRole('form', { name: 'Claude Code' })).toBeVisible();

  // An app the owner turned off is turned away, with the way to turn it back on.
  const owner = await dispatch.client();
  const off = await owner.post('/api/platform/oauth/apps', { id: 'claude-code', allowed: false });
  expect(off.status, off.body).toBe(200);
  await page.goto(`/oauth/authorize?${query}`);
  await expect(page).toHaveURL(/#authorize\?error=app_not_allowed/);
  await expect(
    page.getByText('Dispatch doesn’t accept this app yet. Turn it on under Agents → Connect.', {
      exact: true,
    }),
  ).toBeVisible();
  await page.getByRole('link', { name: 'Agents → Connect', exact: true }).click();
  await expect(page.getByRole('tab', { name: 'Connect', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  const claude = page
    .getByRole('region', { name: 'Apps that may connect', exact: true })
    .getByRole('switch', { name: 'Claude Code', exact: true });
  await expect(claude).not.toBeChecked();
  await claude.click();
  await expect(claude).toBeChecked();
  await page.goto(`/oauth/authorize?${query}`);
  await expect(page).toHaveURL(/#authorize\?request=/);
  await expect(page.getByRole('form', { name: 'Claude Code' })).toBeVisible();

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
  await expect(website).toContainText('Unverified app — it says it is “Route Planner”');
  await expect(website.locator('.agents-sends')).toHaveText('Sends access to: app.example.com');
});
