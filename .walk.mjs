import { chromium } from 'playwright';
import fs from 'node:fs';
const [link, dsp] = process.argv.slice(2);
const out = '/tmp/dispatch-documents-connections/shots/';
const STATE = '/tmp/dispatch-documents-connections/owner.json';
const browser = await chromium.launch({ args: [`--host-resolver-rules=MAP nll.localhost ${new URL(link).hostname}`] });
const options = { viewport: { width: 1280, height: 1000 }, deviceScaleFactor: 1, colorScheme: 'dark' };
if (!fs.existsSync(STATE)) {
  const c = await browser.newContext(options); const p = await c.newPage();
  await p.goto(link); await p.locator('.account-button').waitFor({ timeout: 30000 });
  await c.storageState({ path: STATE }); await c.close();
}
const origin = new URL(link).origin;
if (!fs.existsSync(out + '4-owner-documents.png')) {
const owner = await (await browser.newContext({ ...options, storageState: STATE })).newPage();
const errors = [];
owner.on('response', (r) => { if (r.url().includes('/api/') && r.status() >= 400) errors.push(`${r.status()} ${new URL(r.url()).pathname}`); });
await owner.goto(`${origin}/#dsp/${dsp}/settings?tab=connections`);
await owner.getByRole('heading', { name: 'DSP Connections' }).waitFor();
await owner.waitForTimeout(800);
await owner.screenshot({ path: out + '1-owner-not-connected.png', fullPage: true });
console.log('personal section for platform owner:', await owner.getByRole('heading', { name: 'Personal Connections' }).count());
await owner.getByRole('button', { name: 'Connect Google' }).click();
const ready = owner.getByRole('dialog', { name: 'Documents is ready' });
await ready.waitFor({ timeout: 30000 });
console.log('after connect url:', owner.url().replace(origin, ''));
await owner.screenshot({ path: out + '2-owner-ready.png' });
await ready.getByRole('button', { name: /Add \d+ folders/ }).click();
await ready.waitFor({ state: 'detached' });
await owner.getByText('Storage').waitFor();
await owner.waitForTimeout(800);
await owner.screenshot({ path: out + '3-owner-connected.png', fullPage: true });
await owner.goto(`${origin}/#dsp/${dsp}/documents`);
await owner.getByRole('link', { name: /Safety & Compliance/ }).waitFor();
console.log('team button:', await owner.getByRole('button', { name: /Team access|person|people/ }).count());
await owner.screenshot({ path: out + '4-owner-documents.png' });
console.log('owner api errors:', errors);

}
// The demo member, at their DSP's own address.
const member = await (await browser.newContext(options)).newPage();
const port = new URL(link).port;
await member.goto(`http://nll.localhost:${port}/`);
await member.getByLabel('Email address').fill('member@dispatch.test');
await member.getByLabel('Password', { exact: true }).fill('Dispatch-demo-2026!');
await member.getByRole('button', { name: 'Sign in', exact: true }).click();
await member.locator('.account-button').waitFor({ timeout: 30000 });
await member.goto(`http://nll.localhost:${port}/#settings?tab=connections`);
await member.getByRole('heading', { name: 'Personal Connections' }).waitFor();
await member.waitForTimeout(4000);
console.log('member sees DSP section:', await member.getByRole('heading', { name: 'DSP Connections' }).count());
await member.screenshot({ path: out + '5-member-settings.png', fullPage: true });
const seen = member.waitForResponse((r) => new URL(r.url()).pathname === '/api/dsp/documents');
await member.goto(`http://nll.localhost:${port}/#documents`);
const real = await (await seen).json();
await member.waitForTimeout(3000);
console.log('member documents:', (await member.locator('main').innerText()).slice(0, 160).replace(/\n/g, ' | '));
// The same page for a member whose email isn't a Google account, and one Google hasn't been asked about yet.
for (const [state, name] of [['needs_account', '6-member-gate'], ['pending', '7-member-checking']]) {
  await member.route('**/api/dsp/documents', (route) =>
    route.fulfill({ json: { ...real, me: { state, email: 'jordan@yahoo.com', linked: false } } }),
  );
  await member.reload(); await member.waitForTimeout(1500);
  await member.screenshot({ path: out + name + '.png' });
  await member.unroute('**/api/dsp/documents');
}
await browser.close();
