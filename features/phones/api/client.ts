import { useData, wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import type { PhonePairing } from './generated/PhonePairing.js';
import type { PhoneRow } from './generated/PhoneRow.js';
import type { Phones } from './generated/Phones.js';

export type { PhonePairing } from './generated/PhonePairing.js';
export type { PhoneRow } from './generated/PhoneRow.js';
export type { Phones } from './generated/Phones.js';

// Phones' endpoints, as its card on Settings' Connections calls them.

/** What its card's own error codes say; those `texting` gives are worded in its manifest. */
const wording: Record<string, string> = {
  phones_address_required: 'The DSP needs its own address first.',
  phones_pairing_unavailable: "A code couldn't be made. Try again.",
  // Only the phones' app meets these, though its routes' refusals are each worded.
  phones_pairing_invalid: "That code didn't work. Make a new one.",
  phones_removed: 'This phone was removed.',
};
const call = wordedApi(wording);

/** The DSP's phones, as those who manage its connections see them. */
export const phonesUrl = '/api/dsp/phones';
/** The DSP's phones, read again every `poll` milliseconds, as they check in. */
export const usePhones = (poll: number) => useData<Phones>(phonesUrl, poll);
/** A new code that pairs the next phone, in place of any shown before. */
export const newPairing = () => call<PhonePairing>('/api/dsp/phones/pairing', {});
export const renamePhone = (id: string, name: string) =>
  call<PhoneRow>(`/api/dsp/phones/${encodeURIComponent(id)}/rename`, { name });
/** Removes the phone: its key works no more, and its texts stay. */
export const removePhone = (id: string) =>
  call<{ ok: true }>(`/api/dsp/phones/${encodeURIComponent(id)}/remove`, {});
