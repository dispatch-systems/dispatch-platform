import type { MessageDisplay } from '../../api/index.js';

// Saving a member's choices as they make them. Each change is shown at once, and saved a moment
// after the last (`later`), one save at a time, each through `perform` (which says why one failed
// until the next succeeds): the newest choices made when its turn comes, on the revision the save
// before it answered, and only while the view they were made in is still open, so a DSP switched
// to never gets another's. The choices stay shown as made until the newest of them is saved, or
// its save fails. A save refused because another session saved first (`clash`) drops every
// change not yet saved and reads Settings again (`reread`); no change is taken until what it
// holds is (`reread(revision)`).

/** Runs a save, saying why it failed until another succeeds. */
export type Perform = (work: () => Promise<void>) => Promise<unknown>;

type Waiting = { values: MessageDisplay; version: number; generation: number; view: unknown };

export function autosave({
  save,
  view,
  revision,
  perform,
  later,
  shown,
  clash,
  reread,
}: {
  /** Saves `values` on `revision`, answering the revision after it. */
  save: (values: MessageDisplay, revision: number) => Promise<number>;
  /** Which view is open now. */
  view: () => unknown;
  /** The revision the first save goes on. */
  revision: number;
  perform: Perform;
  /** Runs `then` in a moment, answering what cancels it. */
  later: (then: () => void) => () => void;
  /** Shows the choices as made, or none once Settings holds them. */
  shown: (draft: MessageDisplay | undefined) => void;
  /** Whether a save failed for another session's saving first. */
  clash: (error: unknown) => boolean;
  /** Reads Settings again. */
  reread: () => void;
}) {
  let at = revision;
  let made = 0;
  // Each change a clash drops is of an older generation, never saved.
  let generation = 0;
  let draft: MessageDisplay | undefined;
  let waiting: Waiting | undefined;
  let rereading = false;
  let cancel = () => {};
  let turn: Promise<unknown> = Promise.resolve();
  /** The save of `version` is done: the newest's, Settings holds what's shown. */
  const settled = (version: number) => {
    if (version !== made) return;
    draft = undefined;
    shown(undefined);
  };
  const flush = () => {
    cancel();
    const job = waiting;
    if (!job) return turn;
    waiting = undefined;
    // One dropped since, or made in a view no longer open, isn't saved, and says nothing.
    turn = turn.then(() =>
      job.generation !== generation || view() !== job.view
        ? undefined
        : perform(async () => {
            try {
              at = await save(job.values, at);
            } catch (error) {
              if (clash(error)) {
                generation++;
                waiting = undefined;
                cancel();
                rereading = true;
                reread();
                settled(made);
              } else settled(job.version);
              throw error;
            }
            settled(job.version);
          }),
    );
    return turn;
  };
  return {
    /** The choices as made, while some aren't saved. */
    get draft() {
      return draft;
    },
    /** Settings holds `revision`: saves go on from it, and changes are taken again. */
    reread(revision: number) {
      at = revision;
      rereading = false;
    },
    /** The member's newest choices, in the view open now: whether they were taken. */
    change(values: MessageDisplay) {
      if (rereading) return false;
      draft = values;
      waiting = { values, version: ++made, generation, view: view() };
      shown(values);
      cancel();
      cancel = later(() => void flush());
      return true;
    },
    /** Saves what waits now, once the saves before it are done. */
    flush,
  };
}
