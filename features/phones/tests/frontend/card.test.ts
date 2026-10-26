import assert from 'node:assert/strict';
import test from 'node:test';
import type { DspView } from '../../../../core/accounts/api/index.js';
import { dspPieces } from '../../../../core/collection/frontend/dspPieces.js';
import { installFeatures } from '../../../../core/shell/frontend/runtime/slots.js';
import { feature } from '../../frontend/feature.js';

// The Phones card shows on DSP Connections for whoever manages the DSP's phones, or all its
// connections, while Phones is on, and for nobody else.

const view = (permissions: string[], features = ['phones', 'cortex']) =>
  ({ features, permissions, role: { owner: false } }) as unknown as DspView;
const shown = (of: DspView) => dspPieces(of).some((piece) => piece.id === 'phones');

test('the Phones card shows for those who manage the phones or all connections', () => {
  installFeatures([feature]);
  assert.equal(shown(view(['phones.manage'])), true);
  assert.equal(shown(view(['connections.manage'])), true);
  assert.equal(shown(view([])), false);
  assert.equal(shown(view(['phones.manage'], ['cortex'])), false, 'not while Phones is off');
});
