import { useData, wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import type { DocumentsOverview } from './generated/DocumentsOverview.js';
import type { GoogleSignIn } from './generated/GoogleSignIn.js';
import type { DocumentsFolder } from './generated/DocumentsFolder.js';
import type { DocumentsItem } from './generated/DocumentsItem.js';
import type { NewKind } from './generated/NewKind.js';

export type { DocumentsOverview } from './generated/DocumentsOverview.js';
export type { DocumentsConnection } from './generated/DocumentsConnection.js';
export type { DocumentsFolder } from './generated/DocumentsFolder.js';
export type { DocumentsItem } from './generated/DocumentsItem.js';
export type { ItemKind } from './generated/ItemKind.js';
export type { NewKind } from './generated/NewKind.js';

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
  documents_not_connected: "Documents isn't connected to Google. Refresh the page.",
  documents_item_not_found:
    'That file or folder is gone: someone moved it, trashed it or changed it in Google Drive. Refresh the page.',
  documents_name_invalid: 'Choose a name of up to 200 characters.',
  documents_storage_full:
    'The Google account that holds Documents is out of storage. Free up space in it, or add storage with Google One.',
  documents_too_many_files: 'Documents holds more files than Dispatch can list at once.',
});

/** The DSP's Google connection, and whether it can make one. */
export const useDocumentsOverview = () => useData<DocumentsOverview>('/api/dsp/documents');
/** Starts a sign-in with Google: where the browser goes next. */
export const connectGoogle = () => call<GoogleSignIn>('/api/dsp/documents/connect', {});
/** Finishes the sign-in Google sent the browser back from. */
export const finishGoogle = (state: string, code: string) =>
  call<DocumentsOverview>('/api/dsp/documents/connect/finish', { state, code });
export const disconnectGoogle = () => call<DocumentsOverview>('/api/dsp/documents/disconnect', {});

/** What a folder holds, or with `query`, what a search inside it found. No folder: the top. */
export const documentsFolderUrl = (folder?: string, query?: string) => {
  const params = new URLSearchParams();
  if (folder) params.set('id', folder);
  if (query?.trim()) params.set('q', query.trim());
  const search = params.toString();
  return `/api/dsp/documents/folder${search ? `?${search}` : ''}`;
};
export const useDocumentsFolder = (folder?: string, query?: string) =>
  useData<DocumentsFolder>(
    documentsFolderUrl(folder, query),
    0,
    undefined,
    undefined,
    false,
    (signal) => call<DocumentsFolder>(documentsFolderUrl(folder, query), undefined, signal),
  );
/** Makes a folder, Doc, Sheet or Slides in `folder`, or at the top. */
export const createItem = (folder: string | undefined, kind: NewKind, name: string) =>
  call<DocumentsItem>('/api/dsp/documents/new', { folder: folder ?? null, kind, name });
export const renameItem = (id: string, name: string) =>
  call<DocumentsItem>(`/api/dsp/documents/items/${encodeURIComponent(id)}/rename`, { name });
/** Moves a file or folder, with whatever it holds, to the Google account's trash. */
export const trashItem = (id: string) =>
  call<unknown>(`/api/dsp/documents/items/${encodeURIComponent(id)}/trash`, {});
