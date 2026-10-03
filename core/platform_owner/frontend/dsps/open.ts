import { dspHash, navigate } from '../../../shell/frontend/runtime/navigation.js';

export const open = (dsp: { id: string }) => navigate(dspHash(dsp.id));
