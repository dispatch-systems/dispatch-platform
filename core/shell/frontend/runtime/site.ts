import type { SiteInfo } from '../../../tenancy/api/generated/SiteInfo';

let current: SiteInfo = { kind: 'admin', dsp: null, dspAddress: '' };

/** Which of the server's addresses the dashboard was loaded from, read once as it starts. */
export const site = () => current;
/** At a DSP's address, that DSP: the only one the dashboard opens there. */
export const siteDsp = () => current.dsp?.id;
/** The address of the DSP whose short code is `code`. */
export const dspAddress = (code: string) =>
  current.dspAddress.replace('{code}', code.toLowerCase());
/** Reads which address this is, before anything else asks the server for anything. */
export async function loadSite() {
  const response = await fetch('/api/site', { signal: AbortSignal.timeout(15000) });
  const answer = (await response.json()) as SiteInfo;
  if (!response.ok || !['admin', 'invite', 'dsp'].includes(answer.kind))
    throw new Error('The request could not be completed.');
  current = answer;
}
