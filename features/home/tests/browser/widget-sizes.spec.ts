import fs from 'node:fs';
import path from 'node:path';
import { repositoryRoot as root } from '../../../../tooling/scaffold/common.js';
import {
  test,
  expectEverySize,
  login,
  openDsp,
} from '../../../../core/shell/tests/support/fixtures.js';

/** Every widget a release has shipped, as the slots rules hold them. */
const widgets = fs
  .readFileSync(path.join(root, 'app/tests/rules/widgets.txt'), 'utf8')
  .split('\n')
  .map((line) => line.trim())
  .filter((line) => line && !line.startsWith('#'));

// The seeded DSP has every feature switched on, and its owner holds every permission. What each
// widget shows of a feature's own data, its feature's tests check at every size.
test('every widget fits each size it declares, as the seeded DSP shows it', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const dsp = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await login(page);
  await openDsp(page, dsp.name);
  for (const id of widgets) await expectEverySize(page, dsp.id, id);
});
