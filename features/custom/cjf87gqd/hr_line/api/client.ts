import { linePhones } from '../../../../communications/api/client.js';

// HR Line's endpoints, as its card calls them: Dispatch Phone's calls for a line of its phones,
// at HR Line's own address, for HR Line's alone.

export const hrLinePhonesUrl = '/api/dsp/cjf87gqd-hr-line/phones';
/** HR Line's phones, as those who manage them or all the DSP's connections see them. */
export const hrLinePhones = linePhones(hrLinePhonesUrl);
