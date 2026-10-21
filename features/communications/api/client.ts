import { useCallback } from 'react';
import { useCachedData, useData, wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import { upload } from '../../../core/shell/frontend/runtime/transfer.js';
import type {
  Communications,
  CommunicationsUpdates,
  CommunicationsUpload,
  CommunicationsWaiting,
  Contacts,
  Conversation,
  ConversationMessages,
  Conversations,
  Message,
  Profile,
} from './index.js';

// Communications' endpoints, as its screens call them.

/** What its error codes say. Only its screens make the calls that raise them, so they load with them. */
const api = wordedApi({
  communications_conversation_not_found: 'This conversation is no longer here. Refresh the page.',
  communications_contact_not_found: 'This person is no longer in your Slack workspace.',
  communications_off: 'Communications is off in this Slack workspace.',
  communications_upload_not_found: 'A photo is no longer attached. Add it again.',
  communications_photo_kind: 'Photos must be PNG, JPEG or GIF.',
  communications_file_not_found: 'This file is no longer here.',
  upload_incomplete: "The photo didn't finish uploading. Try again.",
  slack_needs_update: 'This workspace needs updating first.',
  slack_connection_broken: 'Slack stopped accepting the connection. Reconnect the workspace.',
  slack_not_connected: 'This Slack workspace is no longer connected.',
  slack_unreachable: "Slack didn't answer. Try again in a minute.",
  slack_rate_limited: 'Slack is busy. Try again in a minute.',
  slack_refused: "Slack didn't take the message. Try again.",
});

export const communicationsUrl = '/api/dsp/communications';
/** `path` asking `asked`, what's left empty left out: as `team`, for one workspace's alone. */
function address(path: string, asked: Record<string, string | undefined>) {
  const query = new URLSearchParams(
    Object.entries(asked).filter((each): each is [string, string] => Boolean(each[1])),
  ).toString();
  return query ? `${path}?${query}` : path;
}
/** The DSP's conversations, those waiting first, then the answered of every workspace or
 * `team`'s, and its workspaces. */
export const getCommunications = (signal: AbortSignal, team?: string) =>
  api<Communications>(address(communicationsUrl, { team }), undefined, signal);
/** How many conversations wait on an answer, read again every `poll` milliseconds. */
export const waitingUrl = `${communicationsUrl}/waiting`;
export const useCommunicationsWaiting = (poll: number) =>
  useCachedData<CommunicationsWaiting>(waitingUrl, poll);
/** What changed since `after`, and who is typing, once either differs from what the page has. */
export const getCommunicationsUpdates = (after: number, typing: number, signal: AbortSignal) =>
  api<CommunicationsUpdates>(
    `${communicationsUrl}/updates?after=${after}&typing=${typing}`,
    undefined,
    signal,
  );
/** Conversations found by a name typed, misspelt or not, in every workspace or `team`'s. */
export const searchConversations = (typed: string, team?: string) =>
  api<Conversations>(address(`${communicationsUrl}/conversations`, { q: typed, team }));
/** The answered conversations, of every workspace or `team`'s, before the last one shown. */
export const olderConversations = (last: Conversation, team?: string) =>
  api<Conversations>(
    address(`${communicationsUrl}/conversations`, {
      before: `${last.lastAt ?? 0}:${last.id}`,
      team,
    }),
  );
/** A conversation and its latest messages, or those before `first`. */
export const getConversation = (id: string, first?: Message) =>
  api<ConversationMessages>(
    `${communicationsUrl}/conversations/${id}${first ? `?before=${first.ts}` : ''}`,
  );
/** What the profile beside a conversation shows of the member it's with, read as it opens. */
export function useProfile(id: string) {
  const url = `${communicationsUrl}/conversations/${id}/profile`;
  const read = useCallback((signal: AbortSignal) => api<Profile>(url, undefined, signal), [url]);
  return useData<Profile>(url, 0, undefined, undefined, false, read);
}
/** The members of the DSP's workspaces, or of `team`, or those found by a name typed. */
export const getContacts = (typed = '', team?: string) =>
  api<Contacts>(address(`${communicationsUrl}/contacts`, { q: typed, team }));
/** Opens a direct message with a member, to message them first. */
export const openConversation = (teamId: string, userId: string) =>
  api<Conversation>(`${communicationsUrl}/conversations`, { teamId, userId });
/** Sends a message, with the photos added for it. */
export const sendMessage = (id: string, text: string, photos: string[] = []) =>
  api<Message>(`${communicationsUrl}/conversations/${id}/messages`, { text, photos });
/** Marks a conversation answered without a reply. */
export const markDone = (id: string) =>
  api<Conversation>(`${communicationsUrl}/conversations/${id}/done`, {});
/** Says the member is writing in a conversation, for the next few seconds. */
export const sayTyping = (id: string) =>
  api<{ ok: true }>(`${communicationsUrl}/conversations/${id}/typing`, {});
/** Adds a photo for a message not sent yet. */
export const addPhoto = (photo: File, progress?: (sent: number) => void, signal?: AbortSignal) =>
  upload<CommunicationsUpload>(
    `${communicationsUrl}/uploads?name=${encodeURIComponent(photo.name)}&type=${encodeURIComponent(photo.type)}`,
    photo,
    progress,
    signal,
  );
/** Where a photo or file attached to a message is read from. */
export const fileUrl = (id: string) => `${communicationsUrl}/files/${id}`;
