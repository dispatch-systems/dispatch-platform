import { api, useData } from '../../../core/shell/frontend/runtime/api.js';
import type { DocumentsOverview } from './generated/DocumentsOverview.js';
import type { GoogleSignIn } from './generated/GoogleSignIn.js';

export type { DocumentsOverview } from './generated/DocumentsOverview.js';
export type { DocumentsConnection } from './generated/DocumentsConnection.js';

// Documents's endpoints, as its screens call them.

export const documentsOverviewUrl = '/api/dsp/documents';
/** The DSP's Google connection, and who can make one. */
export const useDocumentsOverview = () => useData<DocumentsOverview>(documentsOverviewUrl);
/** Starts a sign-in with Google: where the browser goes next. */
export const connectGoogle = () => api<GoogleSignIn>('/api/dsp/documents/connect', {});
/** Finishes the sign-in Google sent the browser back from. */
export const finishGoogle = (state: string, code: string) =>
  api<DocumentsOverview>('/api/dsp/documents/connect/finish', { state, code });
export const disconnectGoogle = () => api<DocumentsOverview>('/api/dsp/documents/disconnect', {});
