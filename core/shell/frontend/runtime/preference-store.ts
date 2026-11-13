import type { DspView, PreferenceSaved } from '../../../accounts/api/index.js';

// A member's own preferences in the DSPs they open, as each view brings them
// (`DspView.preferences`): each shown at once as they change it and saved behind it, one save at
// a time, from the revision last kept. A save the server finds made from an older revision, as
// when they changed it on another screen meanwhile, is answered with what is kept, and their
// changes are made again on that, so neither screen's replaces the other's.

/** A change: the value made from the one shown, or none to clear it back to its default. */
export type Change<T> = (now: T | undefined) => T | undefined;
/** Saves a value, none clearing it, made from `revision`, through the view `token`: answers none once that view is no longer open. */
export type Post = (
  token: string,
  id: string,
  body: { revision: number; value: unknown },
) => Promise<PreferenceSaved | undefined>;
/** One preference of one DSP's member, as kept and as shown. */
export type Kept = {
  readonly id: string;
  /** The view it was read from, through whose token it is saved. */
  from: DspView;
  /** What the server keeps, as last heard. */
  kept: { revision: number; value?: unknown };
  /** Changes not yet kept, in the order made. */
  changes: Change<unknown>[];
  /** What is kept, with the changes made on it. */
  shown: unknown;
  sending: boolean;
  /** Told why a change couldn't be kept; it stays shown, and goes with the next one. */
  failed?: (error: unknown) => void;
};

/** How many times in a row a save is answered with a newer revision before it fails. */
const RETRIES = 3;

const applied = (value: unknown, changes: readonly Change<unknown>[]) =>
  changes.reduce((now, change) => change(now), value);

export class PreferenceStore {
  private readonly kept = new Map<string, Kept>();
  private readonly listeners = new Set<() => void>();
  constructor(private readonly post: Post) {}

  /**
   * `id` of the DSP `view` is of: as the view brings it, unless changes of the member's are
   * still on their way, which a view opened since never replaces.
   */
  of(view: DspView, id: string): Kept {
    const key = `${view.dsp.id}\n${id}`;
    const known = this.kept.get(key);
    if (known && (known.from === view || known.sending || known.changes.length)) return known;
    const kept = view.preferences[id] ?? { revision: 0 };
    const fresh: Kept = { id, from: view, kept, changes: [], shown: kept.value, sending: false };
    this.kept.set(key, fresh);
    return fresh;
  }

  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => void this.listeners.delete(listener);
  };

  /** Shows `change` at once, and saves it in its turn. */
  change(preference: Kept, change: Change<unknown>) {
    preference.changes.push(change);
    this.show(preference);
    if (!preference.sending) void this.send(preference);
  }

  private show(preference: Kept) {
    preference.shown = applied(preference.kept.value, preference.changes);
    for (const listener of this.listeners) listener();
  }

  private async send(preference: Kept) {
    preference.sending = true;
    let stale = 0;
    while (preference.changes.length) {
      const changes = preference.changes.slice();
      const value = applied(preference.kept.value, changes);
      let saved: PreferenceSaved | undefined;
      try {
        saved = await this.post(preference.from.token, preference.id, {
          revision: preference.kept.revision,
          value: value ?? null,
        });
      } catch (error) {
        preference.failed?.(error);
        break;
      }
      if (!saved) break;
      if (saved.outcome === 'saved') {
        preference.kept = { revision: saved.revision, value };
        preference.changes.splice(0, changes.length);
        stale = 0;
      } else {
        preference.kept = saved.current;
        if (++stale > RETRIES) {
          preference.failed?.(new Error('It changed on another screen. Try again.'));
          this.show(preference);
          break;
        }
      }
      this.show(preference);
    }
    preference.sending = false;
  }
}
