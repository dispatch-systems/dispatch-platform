import type { Conversation, ConversationMessages, Message, Workday } from '../../api/index.js';
import { getConversation } from '../../api/client.js';
import { ApiError } from '../../../../core/shell/frontend/runtime/api.js';
import { canPrefetch } from '../../../../core/shell/frontend/runtime/prefetch.js';
import { messageOf } from '../../../../core/shell/frontend/lib/errors.js';
import type { Changed } from '../inbox/useInbox.js';
import { newer } from '../inbox/inbox.js';
import { fresh, joined } from './messages.js';

// The conversations the page has read, kept while it's open, so one opened again shows at once:
// each as last read, read again only once an update changed it since, and as it opens, the one
// open at once as it changes; and read before it's opened, as a row is pointed at or reached by
// the keyboard, or as one of the first waiting, a few at a time and after what's opened. The
// longest unopened go once there are too many.

/** A conversation as read, and when its latest read answered, by the page's own clock. */
export type Opened = Omit<ConversationMessages, 'messages'> & {
  id: string;
  messages: Message[];
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
  sent: Map<string, number>;
};

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
      if (!watching.size) this.watchers.delete(id);
    };
  }
  /** The latest update, `changed`: what it names, and each conversation whose read shows one it
   * names among whom it's with's others (`elsewhere`), as a driver's work phone, is read again once
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
    try {
      const page = await getConversation(id, shown.messages[0]);
      const now = this.entries.get(id) === entry ? entry.held.opened : undefined;
      if (now && entry.turns.shown === since)
        this.hold(id, {
          ...this.held(id),
          opened: {
            ...now,
            messages: joined(now.messages, page.messages),
            more: page.more,
            workdays: withDays(now.workdays, page.workdays),
          },
        });
    } catch (cause) {
      // Read again only as asked again, never at once.
      if (this.entries.get(id) === entry)
        this.hold(id, { ...entry.held, error: messageOf(cause), olderFailed: true });
    } finally {
      if (this.entries.get(id) === entry) this.hold(id, { ...this.held(id), older: false });
    }
  }
  /** Shows the conversation as one of the page's own writes left it, and a message it sent. */
  show(conversation: Conversation, message?: Message) {
    const entry = this.entries.get(conversation.id);
    const now = entry?.held.opened;
    if (!entry || !now) return;
    if (message) entry.sent.set(message.id, entry.turns.asked);
    this.hold(conversation.id, {
      ...entry.held,
      opened: {
        ...now,
        conversation: newer(now.conversation, conversation),
        messages: message ? joined(now.messages, [message]) : now.messages,
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
    try {
      latest = await getConversation(id, undefined, priority);
    } catch (cause) {
      if (this.entries.get(id) !== entry) return;
      entry.reading = false;
      // Not read as it is: read again as it opens or the next update comes, never at once.
      entry.asked = -1;
      if (cause instanceof ApiError && cause.status === 404)
        this.hold(id, { ...nothing, gone: true });
      else this.hold(id, { ...entry.held, error: messageOf(cause) });
      return;
    }
    if (this.entries.get(id) !== entry) return;
    entry.reading = false;
    if (turn >= entry.turns.shown) {
      entry.turns.shown = turn;
      const before = entry.held.opened;
      const unread = (message: Message) => (entry.sent.get(message.id) ?? 0) >= turn;
      this.hold(id, {
        ...entry.held,
        error: '',
        opened: {
          ...latest,
          id,
          readAt: performance.now(),
          workdays: withDays(before?.workdays ?? [], latest.workdays),
          ...fresh(before, latest, unread),
        },
      });
    }
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
      sent: new Map(),
    };
    this.entries.delete(id);
    this.entries.set(id, entry);
    for (const [kept] of this.entries) {
      if (this.entries.size <= KEPT) break;
      if (!this.watchers.has(kept) && kept !== id) this.forget(kept);
    }
    return entry;
  }
  private forget(id: string) {
    this.entries.delete(id);
    this.changedAt.delete(id);
  }
  private hold(id: string, held: Held) {
    const entry = this.entries.get(id);
    if (!entry) return;
    entry.held = held;
    for (const watch of this.watchers.get(id) ?? []) watch();
  }
}
