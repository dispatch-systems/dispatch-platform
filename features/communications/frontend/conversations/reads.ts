import type { Conversation, ConversationMessages, Message, Workday } from '../../api/index.js';
import type { ConversationWindow } from '../../api/index.js';
import {
  getConversation,
  getConversationWindow,
  type ConversationWindowRequest,
} from '../../api/client.js';
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
  window?: ConversationWindow;
  provisional?: boolean;
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

export type ConversationViewport = {
  anchor?: string;
  visible: readonly string[];
  latest: boolean;
  blocked: boolean;
  serial?: number;
};
export type WriteVersion = { entry: object | undefined; turn: number };
type Move = 'older' | 'newer' | 'latest';
type ReadOwner = { controller: AbortController; move?: Move };

type Entry = {
  viewport?: () => ConversationViewport;
  failed?: Move;
  current?: ReadOwner;
  pendingMove?: Move;
  held: Held;
  /** The latest update the page had as its latest read was asked: one since that names it, or
   * every one, changed it. */
  asked: number;
  /** Its reads in turn, and the latest whose answer shows. */
  turns: { asked: number; shown: number };
  /** Dispatch epochs distinguish reads issued before a write from later authoritative reads. */
  epoch: number;
  observed: Map<string, { turn: number; message: Message }>;
  /** Each message the page sent, by the read asked last as it showed: one asked before then may
   * not hold it yet. */
  sent: Map<string, { turn: number; started?: number; message: Message }>;
  requests: Set<AbortController>;
  olderRead?: AbortController;
};

/** Sends stay separate from the contiguous server window until that window reaches them.
 * An old retry must never become the backward-paging cursor across an unread gap. */
function withSends(
  entry: Entry,
  window: { messages: Message[]; more: boolean },
  read: readonly Message[] = [],
  bounded: ConversationWindow | null | undefined = entry.held.opened?.window,
  observed?: number,
) {
  // A returned row is authoritative, as fresh() has always treated it. Keep its version in
  // the separate record too, so an old send response cannot replay waiting over failed/sent.
  for (const message of read) {
    const pending = entry.sent.get(message.id);
    if (
      pending &&
      (pending.started === undefined || (observed !== undefined && observed > pending.started))
    )
      entry.sent.set(message.id, { ...pending, message });
  }
  if (!entry.sent.size) return window.messages;
  if (bounded) {
    const ids = new Set(window.messages.map((message) => message.id));
    const last = bounded.messages.at(-1);
    const sent = [...entry.sent.values()]
      .map(({ message }) => message)
      .filter(
        (message) =>
          ids.has(message.id) || (!bounded.range.hasAfter && (!last || byTs(message, last) > 0)),
      );
    // Pending receipts are never range endpoints and never bridge a historical gap.
    return joined(window.messages, sent);
  }
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
  constructor(private readonly timeZone: string) {}

  /** The mounted thread supplies current geometry, including interactions begun during a read. */
  viewport(id: string, read: () => ConversationViewport) {
    if (this.disposed) return () => undefined;
    const entry = this.touch(id);
    entry.viewport = read;
    return () => {
      if (entry.viewport === read) entry.viewport = undefined;
    };
  }
  resume(id: string) {
    const entry = this.entries.get(id);
    if (entry?.held.error && !entry.pendingMove) return;
    if (!this.watchers.has(id)) return;
    if (entry?.pendingMove === 'older' && !entry.held.opened?.window) void this.readOlder(id);
    else if (entry?.pendingMove || this.stale(id))
      void this.read(id, undefined, entry?.pendingMove);
  }
  /** Captured before a write, so a delayed receipt cannot overwrite a newer server row. */
  version(id: string): WriteVersion {
    const entry = this.entries.get(id);
    return { entry, turn: entry?.epoch ?? 0 };
  }
  readNewer(id: string) {
    return this.read(id, undefined, this.entries.get(id)?.failed ?? 'newer');
  }
  readLatest(id: string) {
    return this.read(id, undefined, 'latest');
  }

  private disposed = false;
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
        if (entry) {
          // Detach owners before abort: their obsolete finalizers cannot clear a later owner.
          entry.current = undefined;
          entry.olderRead = undefined;
          for (const request of entry.requests) request.abort();
          if (entry.requests.size) entry.asked = -1;
          entry.pendingMove = undefined;
          entry.viewport = undefined;
          entry.held = { ...entry.held, older: false };
        }
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
    if (shown?.window) return this.read(id, undefined, entry?.failed ?? 'older');
    if (!entry || !shown?.more || entry.held.older || !shown.messages[0]) return;
    if (entry.viewport?.().blocked) {
      entry.pendingMove = 'older';
      return;
    }
    entry.pendingMove = undefined;
    // A read of the latest shown before the page comes may have filed some of its messages in
    // another conversation: the page is then from before, and is read again while it's in view.
    const since = entry.turns.shown;
    const observed = ++entry.epoch;
    const controller = new AbortController();
    entry.requests.add(controller);
    entry.olderRead = controller;
    this.hold(id, { ...entry.held, older: true, olderFailed: false });
    const owns = () =>
      this.entries.get(id) === entry &&
      entry.olderRead === controller &&
      !controller.signal.aborted;
    let succeeded = false;
    try {
      const page = await getConversation(id, shown.messages[0], undefined, controller.signal);
      const now = this.entries.get(id) === entry ? entry.held.opened : undefined;
      if (now && owns() && entry.turns.shown === since) {
        if (entry.viewport?.().blocked) {
          entry.pendingMove = 'older';
          return;
        }
        this.observe(entry, page.messages, observed);
        const opened = {
          ...now,
          messages: joined(now.messages, page.messages),
          more: page.more,
          workdays: withDays(now.workdays, page.workdays),
        };
        this.hold(id, {
          ...this.held(id),
          opened: { ...opened, messages: withSends(entry, opened, page.messages, null, observed) },
        });
        succeeded = true;
      }
    } catch (cause) {
      // Read again only as asked again, never at once.
      if (owns()) this.hold(id, { ...entry.held, error: messageOf(cause), olderFailed: true });
    } finally {
      entry.requests.delete(controller);
      const owned = entry.olderRead === controller;
      if (owned) entry.olderRead = undefined;
      if (this.entries.get(id) === entry && owned) {
        this.busy(id, entry);
        this.trim(id);
        if (succeeded && this.watchers.has(id) && this.stale(id)) void this.read(id);
      }
    }
  }
  /** Shows the conversation as one of the page's own writes left it, and a message it sent. */
  show(conversation: Conversation, message?: Message, observed?: WriteVersion) {
    const entry = this.entries.get(conversation.id);
    const now = entry?.held.opened;
    if (!entry || !now || (observed && observed.entry !== entry)) return;
    if (message) {
      const authoritative = entry.observed.get(message.id);
      entry.sent.set(message.id, {
        turn: entry.turns.asked,
        started: observed?.turn,
        message:
          observed && authoritative && authoritative.turn > observed.turn
            ? authoritative.message
            : message,
      });
    }
    this.hold(conversation.id, {
      ...entry.held,
      opened: {
        ...now,
        conversation: newer(now.conversation, conversation),
        messages: message ? withSends(entry, now) : now.messages,
      },
    });
    // A snapshot issued after acknowledgement settles receipt protection, even off-window retries.
    if (message && now.window) {
      entry.asked = -1;
      if (this.watchers.has(conversation.id)) void this.read(conversation.id);
    }
  }

  /** Whether the conversation `id` needs reading: not held, or changed by an update since its
   * latest read was asked; not while one is under way, which reads it again once it answers. */
  private stale(id: string) {
    const entry = this.entries.get(id);
    if (!entry) return true;
    if (entry.current) return false;
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
  private origins(window: ConversationWindow): boolean {
    const origins = window.origins;
    return Boolean(
      origins?.ready &&
      origins.version === 1 &&
      origins.algorithm.startsWith('conversation-origins-1:') &&
      origins.timeZone === this.timeZone &&
      (!window.messages.length || origins.selected) &&
      (!window.latest.length || origins.latest),
    );
  }
  /** Paging busy belongs to the currently attached owners, never to a request's finalizer. */
  private busy(id: string, entry: Entry) {
    const older = Boolean(entry.olderRead || entry.current?.move);
    if (entry.held.older !== older) this.hold(id, { ...entry.held, older });
  }
  private cancelOlder(entry: Entry) {
    const controller = entry.olderRead;
    entry.olderRead = undefined;
    controller?.abort();
  }
  private observe(entry: Entry, rows: readonly Message[], turn: number) {
    for (const message of rows) entry.observed.set(message.id, { turn, message });
  }
  private async read(id: string, priority?: 'low', move?: Move) {
    if (this.disposed) return;
    const entry = this.touch(id);
    const before = entry.held.opened;
    const view = entry.viewport?.();
    if (view?.blocked) {
      entry.asked = -1;
      if (move) entry.pendingMove = move;
      return;
    }
    if (move && move !== 'latest' && (!before?.window || before.provisional)) return;
    if (move === 'older' && !before?.window?.range.hasBefore) return;
    if (move === 'newer' && !before?.window?.range.hasAfter) return;
    if (entry.current && !move) return;
    entry.pendingMove = undefined;
    const rows = before?.window?.messages ?? before?.messages ?? [];
    const anchor =
      rows.find((row) => row.id === view?.anchor) ?? (move === 'newer' ? rows.at(-1) : rows[0]);
    const following =
      move === 'latest' || (!move && (!before || before.provisional || (view?.latest ?? true)));
    const index = anchor ? rows.findIndex((row) => row.id === anchor.id) : 0;
    const visible = rows.filter((row) => view?.visible.includes(row.id));
    const lastVisible = visible.at(-1);
    const span = lastVisible ? Math.max(0, rows.indexOf(lastVisible) - index) : 0;
    // A legacy index is not a position within a 150-row selection. Leave room for its
    // visible suffix; a viewport spanning more than the contract stays on the legacy path.
    const placement = !before?.window ? Math.min(index, 50) : index;
    const leading = Math.max(
      0,
      Math.min(
        149 - Math.min(149, span),
        placement + (move === 'older' ? 50 : move === 'newer' ? -50 : 0),
      ),
    );
    const intent: ConversationWindowRequest =
      !following && anchor ? { anchor: anchor.ts, leading, origins: 1 } : { origins: 1 };
    const turn = ++entry.turns.asked;
    const observed = ++entry.epoch;
    const controller = new AbortController();
    const owner: ReadOwner = { controller, move };
    const previous = entry.current;
    entry.current = owner;
    previous?.controller.abort();
    this.cancelOlder(entry);
    entry.asked = this.at;
    entry.requests.add(controller);
    this.hold(id, {
      ...entry.held,
      older: Boolean(move),
      olderFailed: move ? false : entry.held.olderFailed,
    });
    const owns = () =>
      this.entries.get(id) === entry && entry.current === owner && !controller.signal.aborted;
    let succeeded = false;
    // Called after every transport await, before another request or publication.
    const available = () => {
      if (!owns()) return false;
      const current = entry.viewport?.();
      if (current?.blocked) {
        entry.asked = -1;
        if (move) entry.pendingMove = move;
        return false;
      }
      if (move === 'latest' && current?.serial !== view?.serial) {
        entry.asked = -1;
        succeeded = true;
        return false;
      }
      return true;
    };
    try {
      const window = await getConversationWindow(id, intent, priority, controller.signal);
      if (!available()) return;
      const currentView = entry.viewport?.();
      const moved =
        currentView?.anchor !== view?.anchor ||
        currentView?.serial !== view?.serial ||
        (!move && following && currentView && !currentView.latest);
      const currentRows = entry.held.opened?.messages ?? [];
      const currentVisible = currentRows.filter((row) => currentView?.visible.includes(row.id));
      const firstVisible = currentVisible[0];
      const finalVisible = currentVisible.at(-1);
      const first = window.messages[0];
      const last = window.messages.at(-1);
      // Test positions, not presence: authoritative removal must still be allowed to publish.
      const clipped = Boolean(
        first &&
        last &&
        ((window.range.hasBefore && firstVisible && byTs(firstVisible, first) < 0) ||
          (window.range.hasAfter && finalVisible && byTs(finalVisible, last) > 0)),
      );
      if (moved && clipped) {
        entry.asked = -1;
        succeeded = true; // Reacquire once around the user's current viewport.
        return;
      }
      // Following latest intentionally leaves the old viewport; preserved spans must fit.
      if (this.origins(window) && (following || (span < 150 && !clipped))) {
        // Selected rows, origins, days and independent latest evidence have one publication owner.
        this.cancelOlder(entry);
        for (const [key, sent] of entry.sent) if (sent.turn < turn) entry.sent.delete(key);
        this.observe(entry, [...window.messages, ...window.latest], observed);
        entry.turns.shown = turn;
        const opened: Opened = {
          ...window,
          id,
          window,
          messages: window.messages,
          more: window.range.hasBefore,
          lastRead: window.latest,
          readAt: performance.now(),
        };
        this.hold(id, {
          ...entry.held,
          older: Boolean(move),
          error: '',
          olderFailed: move ? false : entry.held.olderFailed,
          opened: {
            ...opened,
            messages: withSends(
              entry,
              opened,
              [...window.messages, ...window.latest],
              window,
              observed,
            ),
          },
        });
      } else {
        // Capability fallback retains the legacy consumer. Rebuild ready history through the
        // CURRENT viewport; a scroll or a completed older page during these awaits belongs to it.
        let latest: ConversationMessages =
          before?.window || !following
            ? await getConversation(id, undefined, priority, controller.signal)
            : { ...window, more: window.range.hasBefore };
        if (!available()) return;
        const lastRead = latest.messages;
        while (latest.more && latest.messages[0]) {
          const current = entry.held.opened;
          const viewport = entry.viewport?.();
          const target = current?.messages.find((row) => row.id === viewport?.anchor) ?? anchor;
          // Existing eligible legacy history is merged below; it needs no second traversal.
          if (!current?.window || !target || byTs(latest.messages[0], target) <= 0) break;
          const first = latest.messages[0];
          const page = await getConversation(id, first, priority, controller.signal);
          if (!available()) return;
          if (!page.messages.length || byTs(page.messages[0]!, first) >= 0) break;
          latest = {
            ...latest,
            messages: joined(latest.messages, page.messages),
            more: page.more,
            workdays: withDays(latest.workdays, page.workdays),
          };
        }
        if (!available()) return;
        // No await between this current snapshot, its viewport validation and publication.
        const current = entry.held.opened;
        const kept = current?.window ? undefined : current;
        this.cancelOlder(entry);
        for (const [key, sent] of entry.sent) if (sent.turn < turn) entry.sent.delete(key);
        this.observe(entry, latest.messages, observed);
        entry.turns.shown = turn;
        const opened: Opened = {
          ...latest,
          id,
          lastRead,
          readAt: performance.now(),
          workdays: withDays(kept?.workdays ?? [], latest.workdays),
          ...fresh(kept, latest, (message) => entry.sent.has(message.id)),
        };
        this.hold(id, {
          ...entry.held,
          older: Boolean(move),
          error: '',
          opened: {
            ...opened,
            messages: withSends(entry, opened, latest.messages, null, observed),
          },
        });
      }
      if (move) entry.failed = undefined;
      succeeded = true;
    } catch (cause) {
      if (!owns()) return;
      entry.asked = -1;
      entry.failed = move;
      if (cause instanceof ApiError && cause.code === 'communications_conversation_not_found')
        this.hold(id, { ...nothing, gone: true });
      else this.hold(id, { ...entry.held, error: messageOf(cause), olderFailed: Boolean(move) });
    } finally {
      entry.requests.delete(controller);
      if (owns()) {
        entry.current = undefined;
        this.busy(id, entry);
        this.trim(id);
        if (succeeded && this.watchers.has(id) && this.stale(id)) void this.read(id);
      }
    }
  }
  /** The conversation `id`'s entry, as the latest used, made if it isn't held, and those used
   * longest ago that aren't open let go, past as many as it keeps. */
  private touch(id: string) {
    const entry: Entry = this.entries.get(id) ?? {
      held: nothing,
      asked: 0,
      turns: { asked: 0, shown: 0 },
      epoch: 0,
      observed: new Map(),
      sent: new Map<string, { turn: number; started?: number; message: Message }>(),
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
    this.disposed = true;
    this.watchers.clear();
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
    if (!entry || !opened || this.watchers.has(id)) return;
    if (opened.window) {
      if (!opened.window.range.hasAfter && !opened.provisional) return;
      const window = opened.window;
      // The recent seed is explicitly provisional: its old range is never used for navigation.
      const seed = {
        ...window,
        messages: window.latest,
        context: window.latestContext,
        workdays: window.latestWorkdays,
        origins: window.origins && {
          ...window.origins,
          selected: window.origins.latest,
        },
      };
      entry.asked = -1;
      this.hold(id, {
        ...entry.held,
        opened: {
          ...opened,
          window: seed,
          provisional: true,
          messages: seed.messages,
          more: false,
          workdays: seed.workdays,
        },
      });
      return;
    }
    if (opened.messages.length <= CLOSED_MESSAGES) return;
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
    const retained = new Set(
      [...(held.opened?.messages ?? []), ...(held.opened?.lastRead ?? [])].map((row) => row.id),
    );
    for (const key of entry.observed.keys())
      if (!retained.has(key) && !entry.sent.has(key)) entry.observed.delete(key);
    for (const watch of this.watchers.get(id) ?? []) watch();
  }
}
