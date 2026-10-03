import { api, useCachedData } from '../../../core/shell/frontend/runtime/api.js';
import type { DriverDetails, DriverMatch, DriverSource } from './index.js';

// Driver Match's endpoints, as its pages call them.

const driverMatch = '/api/dsp/driver-match';
/** Everyone with a Driver Match code, and the pairs that may be one person. */
export const useDriverMatch = () => useCachedData<DriverMatch>(driverMatch);
export const useDriverDetails = (code: string) =>
  useCachedData<DriverDetails>(`${driverMatch}/drivers/${encodeURIComponent(code)}`);
/** `code`'s IDs move to `into`, whose code both keep from then on. */
export const mergeDrivers = (code: string, into: string) =>
  api<DriverMatch>(`${driverMatch}/merge`, { code, into });
/** One of `code`'s IDs moves to a new person. */
export const splitDriver = (code: string, source: DriverSource, id: string) =>
  api<DriverMatch>(`${driverMatch}/split`, { code, source, id });
export const keepDriversApart = (code: string, other: string) =>
  api<DriverMatch>(`${driverMatch}/apart`, { code, other });
