import { useCachedData, wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import type { DriverMatch, PeopleSource, PersonDetails } from './index.js';

// Driver Match's endpoints, as its page calls them. A decision answers with nothing: the page
// reads itself again.

const driverMatch = '/api/dsp/driver-match';
// Only its own screens raise these, so their words load with them.
const api = wordedApi({
  person_changed: 'This person changed in another session. Refresh Driver Match and try again.',
  person_single_id: 'This person has only one ID, so there is nothing to split off.',
  person_code_unavailable: 'Dispatch could not give the new person a code. Try again.',
});
/** Everyone with a Driver Match code, and the questions waiting. */
export const useDriverMatch = () => useCachedData<DriverMatch>(driverMatch);
export const personPath = (code: string) => `${driverMatch}/people/${encodeURIComponent(code)}`;
export const usePersonDetails = (code: string) => useCachedData<PersonDetails>(personPath(code));
/** `code`'s IDs move to `into`, whose code both keep from then on. */
export const mergePeople = (code: string, into: string) =>
  api(`${driverMatch}/merge`, { code, into });
/** One of `code`'s IDs moves to a new person. */
export const splitOff = (code: string, source: PeopleSource, id: string) =>
  api(`${driverMatch}/split`, { code, source, id });
/** One of `code`'s IDs a match joined is theirs. */
export const confirmLink = (code: string, source: PeopleSource, id: string) =>
  api(`${driverMatch}/confirm`, { code, source, id });
export const keepApart = (code: string, other: string) =>
  api(`${driverMatch}/apart`, { code, other });
