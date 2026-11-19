import { useCallback } from 'react';
import { wordedApi, ApiError } from '../../../core/shell/frontend/runtime/api.js';
import { useCachedData, useData } from '../../../core/shell/frontend/runtime/data-reads.js';
import { dataCache } from '../../../core/shell/frontend/runtime/data-cache.js';
import { upload } from '../../../core/shell/frontend/runtime/transfer.js';
import type {
  Communications,
  CommunicationsUpdates,
  CommunicationsUpload,
  CommunicationsWaiting,
  Contacts,
  Conversation,
  ConversationMessages,
  ConversationWindow,
  Conversations,
  Group,
  GroupChoices,
  GroupEntry,
  GroupMember,
  GroupMessages,
  GroupPeople,
  GifSearch,
  Message,
  MessageGif,
  PhonePairing,
  PhonePairingStatus,
  PhoneRow,
  Phones,
  Profile,
  PushKey,
  QuickReplies,
  SentMessage,
  TextContacts,
  TextPeople,
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
  communications_text_moved: 'Moved to another conversation.',
  communications_routes_over: "Today's routes are over.",
  communications_driver_not_found: "This driver isn't on today's routes.",
  driver_match_phone_taken: "This number is already someone else's.",
  person_changed: 'This person changed. Refresh the page and try again.',
  phones_none: 'No phone is paired to send from.',
  phones_not_found: "That phone isn't paired anymore. Refresh the page.",
  phones_text_not_found: "That text can't be sent again.",
  phones_file_not_found: 'A picture is no longer attached. Add it again.',
  phones_address_required: 'The DSP needs its own address first.',
  phones_pairing_unavailable: "A code couldn't be made. Try again.",
  phones_pairing_invalid: "That code didn't work. Make a new one.",
  phones_removed: 'This phone was removed.',
  phones_off: 'Dispatch Phone is off for this DSP.',
  phones_app_not_found: "The app isn't available yet.",
  communications_group_not_found: 'This group is no longer here. Refresh the page.',
  communications_contact_not_found: 'This person is no longer in your Slack workspace.',
  communications_off: 'Dispatch Phone is off in this Slack workspace.',
  communications_upload_not_found: 'A photo is no longer attached. Add it again.',
  communications_reply_sending: 'This message is still sending.',
  communications_photo_kind: 'Photos must be PNG, JPEG or GIF.',
  communications_file_not_found: 'This file is no longer here.',
  communications_push_off: "Notifications can't reach this browser with Dispatch closed.",
  phones_offline: 'The phone is offline.',
  phones_file_gone: 'This video is no longer on the phone.',
  upload_incomplete: "The photo didn't finish uploading. Try again.",
  slack_needs_update: 'This workspace needs updating first.',
  slack_connection_broken: 'Slack stopped accepting the connection. Reconnect the workspace.',
  slack_not_connected: 'This Slack workspace is no longer connected.',
  slack_unreachable: "Slack didn't answer. Try again in a minute.",
  slack_rate_limited: 'Slack is busy. Try again in a minute.',
  slack_refused: "Slack didn't take the message. Try again.",
};
/** An upload refusal as Communications words it, preserving its API metadata. */
function worded(error: unknown) {
  const words = error instanceof ApiError && wording[error.code];
  return words ? new ApiError(error.code, words, error.status, error.requestId) : error;
}
/** One of its calls, a refusal worded as Communications words it. */
const api = wordedApi(wording);

const communicationsUrl = '/api/dsp/communications';
/** `path` asking `asked`, what's left empty left out: as `team`, for one workspace's alone. */
function address(path: string, asked: Record<string, string | undefined>) {
  const query = new URLSearchParams(
    Object.entries(asked).filter((each): each is [string, string] => Boolean(each[1])),
  ).toString();
  return query ? `${path}?${query}` : path;
}
/** Where a list's conversations are: Slack's, of every workspace or `team`'s, or by text, the
 * numbers' or the work phones'. */
export type Listing = { kind: 'number' | 'work' } | { kind?: undefined; team?: string };
const of = (listing: Listing) => (listing.kind ? { kind: listing.kind } : { team: listing.team });
/** No automatic fallback or cache merge: the answer replaces the requested window. */
export type ConversationWindowRequest = { origins?: 1 } & (
  | { cursor: string; anchor?: never; leading?: never }
  | { anchor: string; leading: number; cursor?: never }
  | { cursor?: never; anchor?: never; leading?: never }
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
/** Where the page searches KLIPY's GIFs, while the DSP has them and the server can. */
export const getGifSearch = () => api<GifSearch>(`${communicationsUrl}/gifs`);
/** Adds the number a conversation texts to the person `code`'s. */
export const addToPerson = (id: string, code: string) =>
  api<Conversation>(`${communicationsUrl}/conversations/${id}/person`, { code });
/** Takes the number a conversation texts off the person `code`, whom it was added to. */
export const removeFromPerson = (id: string, code: string) =>
  api<Conversation>(`${communicationsUrl}/conversations/${id}/person/remove`, { code });
/** Everyone Driver Match knows who hasn't left, whom a number can be added to, read while Save
 * contact is open. */
export const useTextPeople = () => useCachedData<TextPeople>(`${communicationsUrl}/texts/people`);
/** Opens the work conversation of one of today's drivers, to text them first. */
export const textDriver = (driverId: string) =>
  api<Conversation>(`${communicationsUrl}/texts/conversations`, { driverId });
/** Today's Routes: the drivers on a route today, and the work phones nobody held. */
export const todaysRoutesUrl = `${communicationsUrl}/routes`;
export const getTodaysRoutes = (signal?: AbortSignal) =>
  api<TodaysRoutes>(todaysRoutesUrl, undefined, signal);
/** The DSP's quick replies, in their order, as the composer offers them and Settings edits them. */
export const quickRepliesUrl = `${communicationsUrl}/quick-replies`;
export const useQuickReplies = () => useCachedData<QuickReplies>(quickRepliesUrl);
/** Saves the DSP's quick replies, in their order, on the `revision` they were read at. Once saved,
 * they take the place of what was read, so they show at once and the next save goes on their
 * revision, unless another view opened meanwhile. */
export async function saveQuickReplies(replies: string[], revision: number) {
  const session = dataCache.session;
  const saved = await api<QuickReplies>(quickRepliesUrl, { replies, revision });
  if (dataCache.session === session) dataCache.put(quickRepliesUrl, saved);
  return saved;
}
/**
 * How a message carries photos and files, by the way it goes: a photo added before it's sent, and
 * where a photo or file attached to one is read from once it's kept.
 */
export type Attachments = {
  add: (photo: File, signal?: AbortSignal) => Promise<CommunicationsUpload>;
  address: (id: string) => string;
  /** Where the picture of a video's first frame is read from, for a text's video its phone keeps. */
  preview?: (id: string) => string;
  /**
   * Asks for a text's video, from its phone where it keeps it, waiting a while for the phone to
   * send it: whether it's kept.
   */
  fetch?: (id: string, signal?: AbortSignal) => Promise<{ kept: boolean }>;
};
/** Adds `photo` through the upload route at `url`, for a message not sent yet: a refusal worded as
 * its calls' are. */
const adding = (url: string) => (photo: File, signal?: AbortSignal) =>
  upload<CommunicationsUpload>(
    `${url}?name=${encodeURIComponent(photo.name)}&type=${encodeURIComponent(photo.type)}`,
    photo,
    undefined,
    signal,
  ).catch((error: unknown) => {
    throw worded(error);
  });
/** A Slack message's, kept in Dispatch. */
export const slackAttachments: Attachments = {
  add: adding(`${communicationsUrl}/uploads`),
  address: (id) => `${communicationsUrl}/files/${id}`,
};
/** A text's, of the line at `url`, kept with the texts its phones relay. */
const textAttachments = (url: string): Attachments => ({
  add: adding(`${url}/texts/uploads`),
  address: (id) => `${url}/texts/files/${id}`,
  preview: (id) => `${url}/texts/files/${id}/preview`,
  fetch: (id, signal) => api<{ kept: boolean }>(`${url}/texts/files/${id}/fetch`, {}, signal),
});
/**
 * One line's texting at `url`, as its page, its count, its alerts and its banner of phones down
 * call it: Dispatch Phone's own, Slack's among it, or another feature's, from routes of its own, by
 * the same calls and words. Each reads and changes only that line's conversations.
 */
export function lineTexting(url: string) {
  const waitingUrl = `${url}/waiting`;
  const pushUrl = `${url}/push`;
  return {
    /** Where its routes are, which every one begins with. */
    url,
    /** The line's conversations, those waiting first, then the answered, of every workspace or
     * `team`'s on Dispatch Phone's own, and its workspaces. */
    getOverview: (signal: AbortSignal, team?: string) =>
      api<Communications>(address(url, { team }), undefined, signal),
    /** How many conversations wait on an answer, read again every `poll` milliseconds. */
    waitingUrl,
    useWaiting: (poll: number) => useCachedData<CommunicationsWaiting>(waitingUrl, poll),
    /** How many conversations wait, and the latest message each waits on. */
    getWaiting: (signal: AbortSignal) => api<CommunicationsWaiting>(waitingUrl, undefined, signal),
    /** The key a browser subscribes with to be told of new messages with Dispatch closed: none
     * while the server pushes nothing. */
    getPushKey: (signal?: AbortSignal) => api<PushKey>(pushUrl, undefined, signal),
    usePushKey: () => useCachedData<PushKey>(pushUrl),
    /** Keeps this browser's subscription, `endpoint` and its `keys`, as the member's. */
    keepPushing: (endpoint: string, keys: { p256dh: string; auth: string }, signal?: AbortSignal) =>
      api<{ ok: true }>(pushUrl, { endpoint, keys }, signal),
    /** Lets go of the member's browser at `endpoint`. */
    stopPushing: (endpoint: string, signal?: AbortSignal) =>
      api<{ ok: true }>(`${pushUrl}/remove`, { endpoint }, signal),
    /** What changed since `after`, and who is typing, once either differs from what the page has. */
    getUpdates: (after: number, typing: number, signal: AbortSignal) =>
      api<CommunicationsUpdates>(
        `${url}/updates?after=${after}&typing=${typing}`,
        undefined,
        signal,
      ),
    /** Conversations found by a name typed, misspelt or not, or by a number. */
    searchConversations: (typed: string, listing: Listing) =>
      api<Conversations>(address(`${url}/conversations`, { q: typed, ...of(listing) })),
    /** The answered conversations, and groups, before the last one shown. */
    olderConversations: (last: Pick<Conversation, 'id' | 'lastAt'>, listing: Listing) =>
      api<Conversations>(
        address(`${url}/conversations`, {
          before: `${last.lastAt ?? 0}:${last.id}`,
          ...of(listing),
        }),
      ),
    /** A conversation and its latest messages, or those before `first`: at low `priority` when
     * it's read before it's opened. */
    getConversation: (id: string, first?: Message, priority?: 'low', signal?: AbortSignal) =>
      api<ConversationMessages>(
        `${url}/conversations/${id}${first ? `?before=${first.ts}` : ''}`,
        undefined,
        signal,
        priority,
      ),
    /** A complete latest/anchored window or one of its before/after/refresh continuations. */
    getConversationWindow: (
      id: string,
      request?: ConversationWindowRequest,
      priority?: 'low',
      signal?: AbortSignal,
    ) =>
      api<ConversationWindow>(
        address(`${url}/conversations/${encodeURIComponent(id)}/window`, {
          origins: request?.origins === undefined ? undefined : String(request.origins),
          cursor: request?.cursor,
          anchor: request?.anchor,
          leading: request?.leading === undefined ? undefined : String(request.leading),
        }),
        undefined,
        signal,
        priority,
      ),
    /** Sends a message, with the photos added for it, or a GIF chosen in their place on Dispatch
     * Phone's own: it, and the conversation it's in now. */
    sendMessage: (id: string, text: string, photos: string[] = [], gif?: MessageGif) =>
      api<SentMessage>(
        `${url}/conversations/${id}/messages`,
        gif ? { text, photos, gif } : { text, photos },
      ),
    /** Marks a conversation answered without a reply, as far as `seen`, the latest message shown. */
    markDone: (id: string, seen?: Message | null) =>
      api<Conversation>(`${url}/conversations/${id}/done`, seen ? { seen: seen.ts } : {}),
    /** Says the member is writing in a conversation, for the next few seconds. */
    sayTyping: (id: string) => api<{ ok: true }>(`${url}/conversations/${id}/typing`, {}),
    /** Sends again a text that failed. */
    retryText: (id: string, message: string) =>
      api<Message>(`${url}/conversations/${id}/messages/${message}/retry`, {}),
    /** Saves a name for the number a conversation texts. */
    saveContact: (id: string, name: string) =>
      api<Conversation>(`${url}/conversations/${id}/contact`, { name }),
    /** Blocks the number a conversation texts, or unblocks it. */
    blockNumber: (id: string, blocked: boolean) =>
      api<Conversation>(`${url}/conversations/${id}/${blocked ? 'block' : 'unblock'}`, {}),
    /** Opens the conversation with a number, to text it first. */
    textFirst: (number: string) => api<Conversation>(`${url}/texts/conversations`, { number }),
    /** Whom the line can text, or those found by a name or number typed, and the numbers blocked. */
    getTextContacts: (typed = '') =>
      api<TextContacts>(address(`${url}/texts/contacts`, { q: typed })),
    /** A text's photos and files, kept with the texts the line's phones relay. */
    attachments: textAttachments(url),
  };
}
export type LineTexting = ReturnType<typeof lineTexting>;
/** Dispatch Phone's own line's. */
export const dispatchTexting = lineTexting(communicationsUrl);
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
/** Marks done every conversation listed under a group that waits, as far as `seen`, the latest of
 * its messages and answers shown. */
export const markGroupDone = (id: string, seen?: GroupEntry | null) =>
  api<Group>(
    `${communicationsUrl}/groups/${id}/done`,
    seen ? { seen: `${seen.at}:${seen.id}` } : {},
  );
/** Managing one line's phones at `url`, as its card on DSP Connections does: Dispatch Phone's own,
 * or another feature's, from its own card, by the same calls and words. */
export function linePhones(url: string) {
  return {
    /** The line's phones, read again every `poll` milliseconds, as they check in. */
    usePhones: (poll: number) => useData<Phones>(url, poll),
    /** A new code that pairs the line's next phone, in place of any of its shown before. */
    newPairing: () => api<PhonePairing>(`${url}/pairing`, {}),
    /** How the code `id` stands, read again every `poll` milliseconds; nothing without one. */
    usePairing: (id: string | undefined, poll: number) =>
      useData<PhonePairingStatus>(id ? `${url}/pairing/${encodeURIComponent(id)}` : '', poll),
    rename: (id: string, name: string) =>
      api<PhoneRow>(`${url}/${encodeURIComponent(id)}/rename`, { name }),
    /** Removes the phone: its key works no more, and its texts stay. */
    remove: (id: string) => api<{ ok: true }>(`${url}/${encodeURIComponent(id)}/remove`, {}),
  };
}
export type LinePhones = ReturnType<typeof linePhones>;
/** Dispatch Phone's own phones, as those who manage them or all its connections see them. */
export const dispatchPhones = linePhones(`${communicationsUrl}/phones`);
