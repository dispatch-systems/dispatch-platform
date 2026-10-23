import type { MessageDisplay } from '../../api/index.js';

// Saving a member's choices as they make them: one save at a time, each the newest choices made
// when its turn comes, on the revision the one before it answered, and only while the view they
// were made in is still the one open, so a DSP switched to never gets another's.

/** What became of one version of the choices: saved; or not, as `error` says, or dropped, made in a
 * view no longer open. */
export type Settled = { version: number; error?: Error; dropped?: true };

export function choicesSaver({
  save,
  view,
  revision,
}: {
  /** Saves `values` on `revision`, answering the revision after it. */
  save: (values: MessageDisplay, revision: number) => Promise<number>;
  /** Which view is open now. */
  view: () => unknown;
  /** The revision the first save goes on. */
  revision: number;
}) {
  let at = revision;
  let made = 0;
  let waiting: { values: MessageDisplay; version: number; view: unknown } | undefined;
  let turn: Promise<unknown> = Promise.resolve();
  return {
    /** Settings, read again, holds `revision`: the next save goes on from it. */
    reread(revision: number) {
      at = revision;
    },
    /** The member's newest choices, in the view open now: its version, each newer than the last. */
    change(values: MessageDisplay) {
      waiting = { values, version: ++made, view: view() };
      return made;
    },
    /** The newest version made. */
    get made() {
      return made;
    },
    /** Saves what waits, once the saves before it are done: what became of it; with nothing
     * waiting, none. */
    flush(): Promise<Settled | undefined> {
      const job = waiting;
      waiting = undefined;
      if (!job) return Promise.resolve(undefined);
      const done = turn.then(async (): Promise<Settled> => {
        if (view() !== job.view) return { version: job.version, dropped: true };
        try {
          at = await save(job.values, at);
          return { version: job.version };
        } catch (error) {
          return {
            version: job.version,
            error: error instanceof Error ? error : new Error(String(error)),
          };
        }
      });
      turn = done;
      return done;
    },
  };
}
