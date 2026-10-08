import { useData, wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import type { DocumentsOverview } from './generated/DocumentsOverview.js';
import type { GoogleSignIn } from './generated/GoogleSignIn.js';

export type { DocumentsOverview } from './generated/DocumentsOverview.js';
export type { DocumentsConnection } from './generated/DocumentsConnection.js';

// Documents's endpoints, as its screens call them.

/** What its error codes say. Only its page makes the calls that raise them, so they load with it. */
const call = wordedApi({
  google_unavailable: "Google isn't set up for Dispatch yet. Ask Dispatch support to finish it.",
  google_unreachable: "Google didn't answer. Try again in a minute.",
  google_sign_in_failed: "Google didn't finish signing in. Try connecting again.",
  google_drive_not_allowed:
    'Google wasn’t given access to Drive. Connect again, and leave the Drive box checked on Google’s screen.',
  google_email_unverified:
    "That Google account's email address isn't verified. Verify it with Google, or use another account.",
  documents_connect_expired: 'That sign-in took too long or was already used. Connect again.',
  documents_account_mismatch:
    'Reconnect with the Google account Documents was set up with. To use a different account, disconnect Google first.',
  google_connection_broken: 'Google stopped accepting the connection. Reconnect Google.',
});

/** The DSP's Google connection, and whether it can make one. */
export const useDocumentsOverview = () => useData<DocumentsOverview>('/api/dsp/documents');
/** Starts a sign-in with Google: where the browser goes next. */
export const connectGoogle = () => call<GoogleSignIn>('/api/dsp/documents/connect', {});
/** Finishes the sign-in Google sent the browser back from. */
export const finishGoogle = (state: string, code: string) =>
  call<DocumentsOverview>('/api/dsp/documents/connect/finish', { state, code });
export const disconnectGoogle = () => call<DocumentsOverview>('/api/dsp/documents/disconnect', {});
