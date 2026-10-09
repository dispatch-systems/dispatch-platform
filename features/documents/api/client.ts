import { ApiError, useData, wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import { download, picture, upload } from '../../../core/shell/frontend/runtime/transfer.js';
import type { DocumentsOverview } from './generated/DocumentsOverview.js';
import type { GoogleSignIn } from './generated/GoogleSignIn.js';
import type { DocumentsFolder } from './generated/DocumentsFolder.js';
import type { DocumentsItem } from './generated/DocumentsItem.js';
import type { NewKind } from './generated/NewKind.js';
import type { DocumentsTeam } from './generated/DocumentsTeam.js';
import type { MySharing } from './generated/MySharing.js';

export type { DocumentsOverview } from './generated/DocumentsOverview.js';
export type { DocumentsConnection } from './generated/DocumentsConnection.js';
export type { DocumentsFolder } from './generated/DocumentsFolder.js';
export type { DocumentsItem } from './generated/DocumentsItem.js';
export type { ItemKind } from './generated/ItemKind.js';
export type { NewKind } from './generated/NewKind.js';
export type { DocumentsTeam } from './generated/DocumentsTeam.js';
export type { TeamPerson } from './generated/TeamPerson.js';
export type { MySharing } from './generated/MySharing.js';
export type { PickerSetup } from './generated/PickerSetup.js';

// Documents's endpoints, as its screens call them.

/** What its error codes say. Only its page makes the calls that raise them, so they load with it. */
const wording: Record<string, string> = {
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
  documents_person_not_found:
    'They have a Google account now, or left your team. Refresh the list.',
  documents_share_not_found: 'That share is gone already. Refresh the list.',
  documents_picked_elsewhere:
    'Those files were picked as another Google account. Pick them again, signed in to Google as the account that holds Documents.',
  upload_too_large: 'Files can be up to 100 MB.',
  uploads_busy: 'Dispatch is busy with other uploads. Try again in a minute.',
  upload_incomplete: "The file didn't finish uploading. Try again.",
  documents_not_downloadable: "Google doesn't let this kind of file be downloaded.",
  documents_export_too_large:
    'Google downloads Docs, Sheets and Slides as Office files only up to 10 MB. Open it in Google instead.',
};
const call = wordedApi(wording);
/** A file's move to or from the server, its failure worded as the calls' are. */
async function moving<T>(work: Promise<T>) {
  try {
    return await work;
  } catch (error) {
    const worded = error instanceof ApiError && wording[error.code];
    throw worded ? new ApiError(error.code, worded, error.status, error.requestId) : error;
  }
}

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
/** The most one upload takes: 100 MB. */
export const UPLOAD_LIMIT = 100 * 1024 * 1024;
/** Uploads `file` named `name` into `folder`, or the top, telling `progress` how much went. */
export const uploadFile = (
  file: File,
  folder: string | undefined,
  progress?: (sent: number) => void,
  signal?: AbortSignal,
) => {
  const params = new URLSearchParams({ name: file.name });
  if (folder) params.set('folder', folder);
  if (file.type) params.set('type', file.type);
  return moving(
    upload<DocumentsItem>(`/api/dsp/documents/upload?${params}`, file, progress, signal),
  );
};
/** Adds files picked with Google's picker to `folder`, or the top. */
export const addFiles = (files: string[], folder: string | undefined) =>
  call<DocumentsItem[]>('/api/dsp/documents/add', { files, folder: folder ?? null });
/** Saves the file `id`: Google's own Docs, Sheets and Slides as Word, Excel and PowerPoint. */
/** Google's picture of a file, at the address its item names. */
export const thumbnail = (url: string, signal: AbortSignal) => picture(url, signal);
export const downloadItem = (id: string) =>
  moving(download(`/api/dsp/documents/items/${encodeURIComponent(id)}/download`));
export const renameItem = (id: string, name: string) =>
  call<DocumentsItem>(`/api/dsp/documents/items/${encodeURIComponent(id)}/rename`, { name });
/** Moves a file or folder, with whatever it holds, to the Google account's trash. */
export const trashItem = (id: string) =>
  call<unknown>(`/api/dsp/documents/items/${encodeURIComponent(id)}/trash`, {});

/** Who on the team edits in Google, who can't yet, and how full the account is. */
export const documentsTeamUrl = '/api/dsp/documents/team';
export const useDocumentsTeam = () =>
  useData<DocumentsTeam>(documentsTeamUrl, 0, undefined, undefined, false, (signal) =>
    call<DocumentsTeam>(documentsTeamUrl, undefined, signal),
  );
/** Emails a member again how to link a Google account. */
export const emailAgain = (user: string) =>
  call<DocumentsTeam>('/api/dsp/documents/team/email', { user });
/** Takes back a share someone made in Google Drive for someone not on the team. */
export const removeShare = (share: string) =>
  call<DocumentsTeam>('/api/dsp/documents/team/remove', { share });
/** Starts the sign-in that links the member's own Google account: where the browser goes. */
export const linkGoogle = () => call<GoogleSignIn>('/api/dsp/documents/link', {});
/** Finishes it, once Google sent the browser back. */
export const finishLink = (state: string, code: string) =>
  call<MySharing | null>('/api/dsp/documents/link/finish', { state, code });
