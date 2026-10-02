import crypto from 'node:crypto';
import { platformHash } from '../../dashboard/src/app/navigation.js';
import { test, expect, demo, login, signIn } from './fixtures.js';

const claudeCode = 'https://claude.ai/oauth/claude-code-client-metadata';
// Claude Code's own listener on this computer; the test answers for it.
const callback = 'http://localhost:43821/callback';

test('an app signs in with Dispatch: the owner signs in, approves it, then revokes it', async ({
  page,
  request,
  baseURL,
  browser,
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

  // On the dashboard in their own browser, the owner starts connecting Claude Code: copying its
  // command lets apps connect, and the dialog waits for it while the app signs in from `page`.
  const owner = await browser.newContext({ baseURL });
  try {
    await owner.grantPermissions(['clipboard-read', 'clipboard-write'], { origin: baseURL });
    const dashboard = await owner.newPage();
    const signedIn = await dashboard.request.post('/api/auth/login', {
      headers: { origin: baseURL! },
      data: { email: demo.email, password: demo.password },
    });
    expect(signedIn.status()).toBe(200);
    await dashboard.goto(`/${platformHash('agents')}`);
    await expect(dashboard.getByRole('heading', { name: 'No apps connected yet' })).toBeVisible();
    await dashboard.getByRole('button', { name: 'Connect an app', exact: true }).click();
    const connecting = dashboard.getByRole('dialog');
    await connecting.getByRole('button', { name: 'Claude Code', exact: true }).click();
    await connecting.getByRole('button', { name: 'Copy Claude Code command', exact: true }).click();
    await expect(connecting.locator('.agents-connect-status')).toHaveText(
      /^Waiting for Claude Code to connect…\((10:00|9:[0-5]\d) left\)$/,
    );

    // Signed out, the request waits through sign-in on the approval page.
    await page.goto(`/oauth/authorize?${query(state)}`);
    await expect(page).toHaveURL(/#authorize\?request=/);
    const approvalUrl = page.url();
    await signIn(page);
    await expect(page.getByRole('heading', { name: 'Connect an app', level: 1 })).toBeVisible();
    expect(page.url()).toBe(approvalUrl);

    const approval = page.getByRole('form', { name: 'Claude Code' });
    await expect(approval.getByText('Known metadata', { exact: true })).toBeVisible();
    await expect(approval.getByText('this computer', { exact: true })).toBeVisible();
    // Every request asks the owner to approve only what they started.
    await expect(
      approval.getByText(
        'Dispatch recognizes this app’s published metadata, but public app identity is not ' +
          'authenticated. Only approve if you started connecting Claude Code yourself just now. ' +
          'If you didn’t, choose Deny. Access will be sent to an app running on this computer.',
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

    // The owner's dialog sees it connect, and what it reaches; Done shows it in the list.
    await expect(connecting).toHaveAccessibleName('Connect Claude Code');
    const connected = connecting.getByRole('status').filter({ hasText: 'is connected' });
    await expect(connected.getByRole('heading')).toHaveText('Claude Code is connected');
    await expect(connected).toContainText('Start a new Claude Code session to use Dispatch.');
    await expect(connected.locator('.agents-tag')).toHaveText(['1 DSP', 'Essential tools']);
    await connecting.getByRole('button', { name: 'Done', exact: true }).click();
    await expect(connecting).toHaveCount(0);

    // An answered request can't be answered again: answering it let go of this browser.
    await page.goto(approvalUrl);
    await expect(
      page.getByText(
        /^This approval isn’t open in this browser, or it has expired\. Start connecting again from your app, and approve it in the browser that opens\./,
      ),
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
    await dashboard.getByRole('tab', { name: 'Keys', exact: true }).click();
    await expect(dashboard.getByText('No keys yet')).toBeVisible();
    await dashboard.getByRole('tab', { name: 'Apps', exact: true }).click();
    const row = dashboard.getByRole('row').filter({ hasText: 'Laptop – Claude Code app' });
    await expect(row).toContainText('Claude Code');
    await expect(row).toContainText('Known metadata');
    await expect(row).toContainText('Northline Logistics');
    await expect(row).toContainText('Essential tools');
    // An app with recognized metadata shows its own logo.
    await expect(row.locator('img')).toHaveAttribute('src', /claude-[\w-]+\.png$/);
    await row.getByRole('button', { name: 'Revoke Laptop – Claude Code app', exact: true }).click();
    await dashboard
      .getByRole('dialog', { name: 'Revoke Laptop – Claude Code app?' })
      .getByRole('button', { name: 'Revoke app', exact: true })
      .click();
    await expect(
      dashboard.getByRole('button', { name: 'Show 1 revoked or expired app' }),
    ).toBeVisible();
    expect((await request.get('/api/v1/whoami', { headers: bearer })).status()).toBe(401);
  } finally {
    await owner.close();
  }
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
