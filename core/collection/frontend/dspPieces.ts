import type { DspView } from '../../accounts/api/index.js';
import { can } from '../../shell/frontend/runtime/permissions.js';
import { connectionPieces } from '../../shell/frontend/runtime/slots.js';

/**
 * The cards features add to DSP Connections that the member sees: every one, for those who
 * manage the DSP's connections, and for others those whose card says they see it.
 */
export const dspPieces = (view?: DspView) =>
  connectionPieces('dsp', view).filter(
    (piece) => can(view, 'connections.manage') || piece.others?.(view),
  );
