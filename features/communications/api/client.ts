import { useCallback } from 'react';
import {
  api as call,
  ApiError,
  useCachedData,
  useData,
} from '../../../core/shell/frontend/runtime/api.js';
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
  Group,
  GroupChoices,
  GroupEntry,
  GroupMember,
  GroupMessages,
  GroupPeople,
  Message,
  PhonePairing,
  PhonePairingStatus,
  PhoneRow,
  Phones,
  Profile,
  SentMessage,
  TextContacts,
  TodaysRoutes,
} from './index.js';

// Communications' endpoints, as its screens and its phones' card on DSP Connections call them.

/**
 * What its error codes say, which only its screens make the calls that raise, so they load with
 * them: the page's, and its phones' card's, including those only the phones' app meets at their
 * own address, though each of its routes' refusals is worded.
 */
const wording: Readonly<Record<string, string>> = {
  communications_conversation_not_found: 'This conversation is no longer here. Refresh the page.',
  communications_message_not_found: 'This text is no longer here. Refresh the page.',
  communications_no_work_phone: 'This driver has no work phone now.',
  communications_work_phone_changed: 'This driver has another work phone now.',
  communications_routes_over: "Today's routes are over.",
  communications_driver_not_found: "This driver isn't on today's routes.",
  phones_none: 'No phone is paired to send from.',
  phones_not_found: "That phone isn't paired anymore. Refresh the page.",
  phones_text_not_found: "That text can't be sent again.",
  phones_file_not_found: 'A picture is no longer attached. Add it again.',
  phones_address_required: 'The DSP needs its own address first.',
  phones_pairing_unavailable: "A code couldn't be made. Try again.",
  phones_pairing_invalid: "That code didn't work. Make a new one.",
  phones_removed: 'This phone was removed.',
  phones_off: 'Communications is off for this DSP.',
  communications_group_not_found: 'This group is no longer here. Refresh the page.',
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
};
/** A refusal as Communications words it, its code, status and request ID as they were. */
function worded(error: unknown) {
  const words = error instanceof ApiError && wording[error.code];
  return words ? new ApiError(error.code, words, error.status, error.requestId) : error;
}
/** One of its calls, a refusal worded as Communications words it. */
const api = <T>(...asked: Parameters<typeof call>) =>
  call<T>(...asked).catch((error: unknown) => {
    throw worded(error);
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
/** How many conversations wait, and the latest message each waits on. */
export const getWaiting = (signal: AbortSignal) =>
  api<CommunicationsWaiting>(waitingUrl, undefined, signal);
/** What changed since `after`, and who is typing, once either differs from what the page has. */
export const getCommunicationsUpdates = (after: number, typing: number, signal: AbortSignal) =>
  api<CommunicationsUpdates>(
    `${communicationsUrl}/updates?after=${after}&typing=${typing}`,
    undefined,
    signal,
  );
/** Where a list's conversations are: Slack's, of every workspace or `team`'s, or by text, the
 * numbers' or the work phones'. */
export type Listing = { kind: 'number' | 'work' } | { kind?: undefined; team?: string };
const of = (listing: Listing) => (listing.kind ? { kind: listing.kind } : { team: listing.team });
/** Conversations found by a name typed, misspelt or not, or by a number. */
export const searchConversations = (typed: string, listing: Listing) =>
  api<Conversations>(address(`${communicationsUrl}/conversations`, { q: typed, ...of(listing) }));
/** The answered conversations, and groups, before the last one shown. */
export const olderConversations = (last: Pick<Conversation, 'id' | 'lastAt'>, listing: Listing) =>
  api<Conversations>(
    address(`${communicationsUrl}/conversations`, {
      before: `${last.lastAt ?? 0}:${last.id}`,
      ...of(listing),
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
/** Sends a message, with the photos added for it: it, and the conversation it's in now. */
export const sendMessage = (id: string, text: string, photos: string[] = []) =>
  api<SentMessage>(`${communicationsUrl}/conversations/${id}/messages`, { text, photos });
/** Marks a conversation answered without a reply. */
export const markDone = (id: string) =>
  api<Conversation>(`${communicationsUrl}/conversations/${id}/done`, {});
/** Says the member is writing in a conversation, for the next few seconds. */
export const sayTyping = (id: string) =>
  api<{ ok: true }>(`${communicationsUrl}/conversations/${id}/typing`, {});
/** Sends again a text that failed. */
export const retryText = (id: string, message: string) =>
  api<Message>(`${communicationsUrl}/conversations/${id}/messages/${message}/retry`, {});
/** Saves a name for the number a conversation texts. */
export const saveContact = (id: string, name: string) =>
  api<Conversation>(`${communicationsUrl}/conversations/${id}/contact`, { name });
/** Blocks the number a conversation texts, or unblocks it. */
export const blockNumber = (id: string, blocked: boolean) =>
  api<Conversation>(
    `${communicationsUrl}/conversations/${id}/${blocked ? 'block' : 'unblock'}`,
    {},
  );
/** Opens the conversation with a number, to text it first. */
export const textFirst = (number: string) =>
  api<Conversation>(`${communicationsUrl}/texts/conversations`, { number });
/** Opens the work conversation of one of today's drivers, to text them first. */
export const textDriver = (driverId: string) =>
  api<Conversation>(`${communicationsUrl}/texts/conversations`, { driverId });
/** Today's Routes: the drivers on a route today, and the work phones nobody held. */
export const todaysRoutesUrl = `${communicationsUrl}/routes`;
export const getTodaysRoutes = (signal?: AbortSignal) =>
  api<TodaysRoutes>(todaysRoutesUrl, undefined, signal);
/** Files a work phone nobody held under one of today's drivers: their conversation. */
export const pickDriver = (id: string, driverId: string) =>
  api<Conversation>(`${communicationsUrl}/conversations/${id}/driver`, { driverId });
/** Whom the DSP can text, or those found by a name or number typed, and the numbers blocked. */
export const getTextContacts = (typed = '') =>
  api<TextContacts>(address(`${communicationsUrl}/texts/contacts`, { q: typed }));
/**
 * How a message carries photos and files, by the way it goes: a photo added before it's sent, and
 * where a photo or file attached to one is read from once it's kept.
 */
export type Attachments = {
  add: (photo: File, signal?: AbortSignal) => Promise<CommunicationsUpload>;
  address: (id: string) => string;
};
/** Adds `photo` through the upload route `path`, for a message not sent yet: a refusal worded as
 * its calls' are. */
const adding = (path: string) => (photo: File, signal?: AbortSignal) =>
  upload<CommunicationsUpload>(
    `${communicationsUrl}/${path}?name=${encodeURIComponent(photo.name)}&type=${encodeURIComponent(photo.type)}`,
    photo,
    undefined,
    signal,
  ).catch((error: unknown) => {
    throw worded(error);
  });
/** A Slack message's, kept in Dispatch. */
export const slackAttachments: Attachments = {
  add: adding('uploads'),
  address: (id) => `${communicationsUrl}/files/${id}`,
};
/** A text's, kept with the texts the DSP's phones relay. */
export const textAttachments: Attachments = {
  add: adding('texts/uploads'),
  address: (id) => `${communicationsUrl}/texts/files/${id}`,
};
/** The address of the workspace `team`'s logo, at its `version`, which it carries. */
export const workspaceLogoUrl = (team: string, version: string) =>
  `${communicationsUrl}/workspaces/${encodeURIComponent(team)}/logo?v=${version}`;
/** Whom a group message can go to, and the groups to choose from. */
export const getGroupChoices = () => api<GroupChoices>(`${communicationsUrl}/groups/choices`);
/** Messages a group of `people`, named `name`. */
export const createGroup = (
  name: string,
  people: readonly GroupMember[],
  text: string,
  photos: string[] = [],
) => api<Group>(`${communicationsUrl}/groups`, { name, people, text, photos });
/** A group and its latest messages and answers, or those before `first`. */
export const getGroup = (id: string, first?: GroupEntry) =>
  api<GroupMessages>(
    `${communicationsUrl}/groups/${id}${first ? `?before=${first.at}:${first.id}` : ''}`,
  );
/** Who a group went to, read as its people open, and again each time `refresh` changes. */
export function useGroupPeople(id: string, refresh: string) {
  const url = `${communicationsUrl}/groups/${id}/people`;
  const read = useCallback(
    (signal: AbortSignal) => api<GroupPeople>(url, undefined, signal),
    [url],
  );
  return useData<GroupPeople>(url, 0, refresh, undefined, false, read);
}
/** Sends a group another message, to the same people. */
export const messageGroup = (id: string, text: string, photos: string[] = []) =>
  api<GroupEntry>(`${communicationsUrl}/groups/${id}/messages`, { text, photos });
/** Marks done every conversation listed under a group that waits. */
export const markGroupDone = (id: string) =>
  api<Group>(`${communicationsUrl}/groups/${id}/done`, {});
/** The DSP's phones, as those who manage them or all its connections see them. */
export const phonesUrl = `${communicationsUrl}/phones`;
/** The DSP's phones, read again every `poll` milliseconds, as they check in. */
export const usePhones = (poll: number) => useData<Phones>(phonesUrl, poll);
/** A new code that pairs the next phone, in place of any shown before. */
export const newPairing = () => api<PhonePairing>(`${phonesUrl}/pairing`, {});
/** How the code `id` stands, read again every `poll` milliseconds; nothing without one. */
export const usePairing = (id: string | undefined, poll: number) =>
  useData<PhonePairingStatus>(id ? `${phonesUrl}/pairing/${encodeURIComponent(id)}` : '', poll);
export const renamePhone = (id: string, name: string) =>
  api<PhoneRow>(`${phonesUrl}/${encodeURIComponent(id)}/rename`, { name });
/** Removes the phone: its key works no more, and its texts stay. */
export const removePhone = (id: string) =>
  api<{ ok: true }>(`${phonesUrl}/${encodeURIComponent(id)}/remove`, {});
