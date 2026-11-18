import { linePhones, lineTexting } from '../../../../communications/api/client.js';

// HR Line's endpoints, as its page and its card call them: Dispatch Phone's calls for a line of its
// phones, at HR Line's own address, for HR Line's alone.

const hrLineUrl = '/api/dsp/cjf87gqd-hr-line';
export const hrLinePhonesUrl = `${hrLineUrl}/phones`;
/** HR Line's phones, as those who manage them or all the DSP's connections see them. */
export const hrLinePhones = linePhones(hrLinePhonesUrl);
/** HR Line's texting, as its page, its count, its alerts and its banner of phones down read it. */
export const hrLineTexting = lineTexting(hrLineUrl);
