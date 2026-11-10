import type { Conversation, ConversationMessages, Message, Workday } from '../../api/index.js';
import { getConversation } from '../../api/client.js';
import { ApiError } from '../../../../core/shell/frontend/runtime/api.js';
import { canPrefetch } from '../../../../core/shell/frontend/runtime/prefetch.js';
import { messageOf } from '../../../../core/shell/frontend/lib/errors.js';
import type { Changed } from '../inbox/useInbox.js';
import { newer } from '../inbox/inbox.js';
import { byTs, fresh, joined } from './messages.js';

// The conversations the page has read, kept while it's open, so one opened again shows at once:
// each as last read, read again only once an update changed it since, and as it opens, the one
// open at once as it changes; and read before it's opened, as a row is pointed at or reached by
// the keyboard, or as one of the first waiting, a few at a time and after what's opened. The
// longest unopened go once there are too many.

/** A conversation as read, and when its latest read answered, by the page's own clock; and the
 * messages that read brought, as read (`lastRead`), never those of older pages read after it. */
export type Opened = Omit<ConversationMessages, 'messages'> & {
  id: string;
  messages: Message[];
  lastRead: readonly Message[];
  readAt: number;
};
/** A conversation as the page holds it: as read, why it couldn't be, or that it's no longer there;
 * and whether a read of older ones is under way, or the latest failed. */
export type Held = {
  opened?: Opened;
  error: string;
  gone: boolean;
  older: boolean;
  olderFailed: boolean;
};

/** How many conversations it keeps: a dispatcher's morning's worth, each at most a page or a few. */
const KEPT = 40;
/** History retained while a conversation is closed; older pages remain available from the server. */
const CLOSED_MESSAGES = 150;
/** How many reads before opening run at once, and how many wait, the latest asked kept, as core's
 * preloads do. */
const WARMING = 2;
const WARM_WAITING = 6;
const nothing: Held = {
  opened: undefined,
  error: '',
  gone: false,
  older: false,
  olderFailed: false,
};

type Entry = {
  held: Held;
  /** The latest update the page had as its latest read was asked: one since that names it, or
   * every one, changed it. */
  asked: number;
  /** Its reads in turn, and the latest whose answer shows. */
  turns: { asked: number; shown: number };
  /** Whether a read of its latest is under way. */
  reading: boolean;
  /** Each message the page sent, by the read asked last as it showed: one asked before then may
   * not hold it yet. */
  sent: Map<string, { turn: number; message: Message }>;
  requests: Set<AbortController>;
  olderRead?: AbortController;
};

/** Sends stay separate from the contiguous server window until that window reaches them.
 * An old retry must never become the backward-paging cursor across an unread gap. */
function withSends(
  entry: Entry,
  window: { messages: Message[]; more: boolean },
  read: readonly Message[] = [],
) {
  // A returned row is authoritative, as fresh() treats it. Keep its version in the separate
  // record too, so an old send response cannot replay waiting over failed/sent.
  for (const message of read) {
    const pending = entry.sent.get(message.id);
    if (pending) entry.sent.set(message.id, { ...pending, message });
  }
  if (!entry.sent.size) return window.messages;
  const first = window.more ? window.messages[0] : undefined;
  const sent = [...entry.sent.values()]
    .map(({ message }) => message)
    .filter((message) => !first || byTs(message, first) >= 0);
  return joined(window.messages, sent);
}

/** Whether reading before opening may go on: the page in view, on a connection that allows it. */
const warmable = () => canPrefetch() && !document.hidden;

/** `kept` workdays with those `read`, each day once, as last read. */
const withDays = (kept: readonly Workday[], read: readonly Workday[]) => [
  ...kept.filter((day) => !read.some((each) => each.day === day.day)),
  ...read,
];

export class ConversationReads {
  private readonly entries = new Map<string, Entry>();
  private readonly watchers = new Map<string, Set<() => void>>();
  /** The latest update the page has had, and when each conversation, or every one, last changed. */
  private at = 0;
  private everyAt = 0;
  private readonly changedAt = new Map<string, number>();
  /** The conversations to read before they're opened, in the order asked, and how many are read. */
  private readonly warming = new Set<string>();
  private warmed = 0;

  /** The conversation `id` as held. */
  held(id: string): Held {
    return this.entries.get(id)?.held ?? nothing;
  }
  /** Calls `watch` as the conversation `id` changes, until what this answers is called. Watched,
   * it's the one open: read again as soon as an update changes it. */
  watch(id: string, watch: () => void) {
    const watching = this.watchers.get(id) ?? new Set();
    watching.add(watch);
    this.watchers.set(id, watching);
    return () => {
      watching.delete(watch);
      if (!watching.size) {
        this.watchers.delete(id);
        const entry = this.entries.get(id);
        entry?.olderRead?.abort();
        this.trim(id);
      }
    };
  }
  /** The latest update, `changed`: what it names, and each conversation whose read shows one it
   * names among whom it's with's others (`elsewhere`), shown or hidden, as a driver's work phone, is read again once
   * it's opened, and the one open now. */
  change(changed: Changed) {
    if (changed.at <= this.at) return;
    this.at = changed.at;
    if (changed.ids === 'all') this.everyAt = changed.at;
    else {
      const named = new Set(changed.ids);
      for (const [id, entry] of this.entries) {
        const others = entry.held.opened?.elsewhere ?? [];
        if (named.has(id) || others.some((each) => named.has(each.conversationId ?? '')))
          this.changedAt.set(id, changed.at);
      }
    }
    for (const id of this.watchers.keys()) if (this.stale(id)) void this.read(id);
  }
  /** Opens the conversation `id`: as held, at once, and read, unless no update changed it since
   * it was. */
  open(id: string) {
    const entry = this.touch(id);
    if (!this.stale(id)) return;
    if (entry.held.error) this.hold(id, { ...entry.held, error: '' });
    void this.read(id);
  }
  /** Reads the conversation `id` before it's opened, unless it's held as it is, or a read of it is
   * under way: optional, so not on a slow or metered connection, at low priority, and a few at a
   * time, those asked longest ago left once too many wait. */
  warm(id: string) {
    if (!warmable() || !this.stale(id)) return;
    this.warming.delete(id);
    this.warming.add(id);
    for (const waiting of this.warming) {
      if (this.warming.size <= WARM_WAITING) break;
      this.warming.delete(waiting);
    }
    this.warmNext();
  }
  /** Reads the fifty messages before the first shown. */
  async readOlder(id: string) {
    const entry = this.entries.get(id);
    const shown = entry?.held.opened;
    if (!entry || !shown?.more || entry.held.older || !shown.messages[0]) return;
    this.hold(id, { ...entry.held, older: true, olderFailed: false });
    // A read of the latest shown before the page comes may have filed some of its messages in
    // another conversation: the page is then from before, and is read again while it's in view.
    const since = entry.turns.shown;
    const controller = new AbortController();
    entry.requests.add(controller);
    entry.olderRead = controller;
    try {
      const page = await getConversation(id, shown.messages[0], undefined, controller.signal);
      const now = this.entries.get(id) === entry ? entry.held.opened : undefined;
      if (now && !controller.signal.aborted && entry.turns.shown === since) {
        const opened = {
          ...now,
          messages: joined(now.messages, page.messages),
          more: page.more,
          workdays: withDays(now.workdays, page.workdays),
        };
        this.hold(id, {
          ...this.held(id),
          opened: { ...opened, messages: withSends(entry, opened, page.messages) },
        });
      }
    } catch (cause) {
      // Read again only as asked again, never at once.
      if (this.entries.get(id) === entry && !controller.signal.aborted)
        this.hold(id, { ...entry.held, error: messageOf(cause), olderFailed: true });
    } finally {
      entry.requests.delete(controller);
      if (entry.olderRead === controller) entry.olderRead = undefined;
      if (this.entries.get(id) === entry) {
        this.hold(id, { ...this.held(id), older: false });
        this.trim(id);
      }
    }
  }
  /** Shows the conversation as one of the page's own writes left it, and a message it sent. */
  show(conversation: Conversation, message?: Message) {
    const entry = this.entries.get(conversation.id);
    const now = entry?.held.opened;
    if (!entry || !now) return;
    if (message) entry.sent.set(message.id, { turn: entry.turns.asked, message });
    this.hold(conversation.id, {
      ...entry.held,
      opened: {
        ...now,
        conversation: newer(now.conversation, conversation),
        messages: message ? withSends(entry, now) : now.messages,
      },
    });
  }

  /** Whether the conversation `id` needs reading: not held, or changed by an update since its
   * latest read was asked; not while one is under way, which reads it again once it answers. */
  private stale(id: string) {
    const entry = this.entries.get(id);
    if (!entry) return true;
    if (entry.reading) return false;
    if (!entry.held.opened) return true;
    return entry.asked < Math.max(this.everyAt, this.changedAt.get(id) ?? 0);
  }
  /** Reads the next conversations waiting to be read before they're opened, while too few are. */
  private warmNext() {
    // Hidden, or on a connection that's become slow or metered, those waiting are let go: pointing
    // at a row or the next update asks again.
    if (!warmable()) return this.warming.clear();
    for (const id of this.warming) {
      if (this.warmed >= WARMING) return;
      this.warming.delete(id);
      if (!this.stale(id)) continue;
      this.warmed++;
      void this.read(id, 'low').finally(() => {
        this.warmed--;
        this.warmNext();
      });
    }
  }
  private async read(id: string, priority?: 'low') {
    const entry = this.touch(id);
    const turn = ++entry.turns.asked;
    entry.reading = true;
    entry.asked = this.at;
    let latest: ConversationMessages;
    const controller = new AbortController();
    entry.requests.add(controller);
    try {
      latest = await getConversation(id, undefined, priority, controller.signal);
    } catch (cause) {
      if (this.entries.get(id) !== entry || controller.signal.aborted) return;
      entry.reading = false;
      // Not read as it is: read again as it opens or the next update comes, never at once.
      entry.asked = -1;
      if (cause instanceof ApiError && cause.status === 404)
        this.hold(id, { ...nothing, gone: true });
      else this.hold(id, { ...entry.held, error: messageOf(cause) });
      return;
    } finally {
      entry.requests.delete(controller);
    }
    if (this.entries.get(id) !== entry || controller.signal.aborted) return;
    entry.reading = false;
    if (turn >= entry.turns.shown) {
      entry.turns.shown = turn;
      const before = entry.held.opened;
      // A read asked after a send no longer needs protection against an earlier snapshot.
      for (const [id, sent] of entry.sent) if (sent.turn < turn) entry.sent.delete(id);
      const unread = (message: Message) => entry.sent.has(message.id);
      const opened = {
        ...latest,
        id,
        lastRead: latest.messages,
        readAt: performance.now(),
        workdays: withDays(before?.workdays ?? [], latest.workdays),
        ...fresh(before, latest, unread),
      };
      this.hold(id, {
        ...entry.held,
        error: '',
        opened: { ...opened, messages: withSends(entry, opened, latest.messages) },
      });
    }
    this.trim(id);
    // Changed while it was read, and open: read again.
    if (this.watchers.has(id) && this.stale(id)) void this.read(id);
  }
  /** The conversation `id`'s entry, as the latest used, made if it isn't held, and those used
   * longest ago that aren't open let go, past as many as it keeps. */
  private touch(id: string) {
    const entry = this.entries.get(id) ?? {
      held: nothing,
      asked: 0,
      turns: { asked: 0, shown: 0 },
      reading: false,
      sent: new Map<string, { turn: number; message: Message }>(),
      requests: new Set<AbortController>(),
    };
    this.entries.delete(id);
    this.entries.set(id, entry);
    for (const [kept] of this.entries) {
      if (this.entries.size <= KEPT) break;
      if (!this.watchers.has(kept) && kept !== id) this.forget(kept);
    }
    return entry;
  }
  /** Releases reads and queued warming when this DSP or page is left. Late answers are ignored. */
  dispose() {
    this.warming.clear();
    for (const id of this.entries.keys()) this.forget(id);
  }
  private forget(id: string) {
    for (const request of this.entries.get(id)?.requests ?? []) request.abort();
    this.entries.delete(id);
    this.changedAt.delete(id);
  }
  /** The open conversation keeps its history and scroll position. A closed one keeps a recent
   * contiguous window; pending sends remain separate until older pages reach them. */
  private trim(id: string) {
    const entry = this.entries.get(id);
    const opened = entry?.held.opened;
    if (!entry || !opened || this.watchers.has(id) || opened.messages.length <= CLOSED_MESSAGES)
      return;
    const messages = opened.messages.slice(-CLOSED_MESSAGES);
    // A workday is local to its DSP. UTC and its neighbouring days cover every time zone while
    // retaining only context for the messages still held.
    const days = new Set(
      messages.flatMap((message) =>
        [-1, 0, 1].map((offset) =>
          new Date(message.at + offset * 86_400_000).toISOString().slice(0, 10),
        ),
      ),
    );
    this.hold(id, {
      ...entry.held,
      opened: {
        ...opened,
        messages,
        more: opened.more || messages.length < opened.messages.length,
        workdays: opened.workdays.filter((day) => days.has(day.day)),
      },
    });
  }
  private hold(id: string, held: Held) {
    const entry = this.entries.get(id);
    if (!entry) return;
    entry.held = held;
    for (const watch of this.watchers.get(id) ?? []) watch();
  }
}
