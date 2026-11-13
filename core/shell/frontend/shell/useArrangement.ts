import { useLayoutEffect, useRef, useState } from 'react';
import type { DspView, SidebarArrangement } from '../../../accounts/api/index.js';
import { messageOf } from '../lib/errors.js';
import { arrangeSidebar } from '../runtime/endpoints.js';
import { useFeedback } from '../runtime/feedback.js';

const unarranged: SidebarArrangement = { order: [], hidden: [] };

/**
 * How the member arranged the sidebar of the DSP open, and the way to arrange it anew. What
 * they arrange shows at once and is kept behind it, one save at a time, the latest last, so
 * nothing waits on the server; a save that fails says why and leaves the sidebar as arranged.
 */
export function useArrangement(view: DspView | undefined) {
  const { fail } = useFeedback();
  const dsp = view?.dsp.id;
  // What they arranged since the view was opened, which the view itself doesn't say.
  const [own, setOwn] = useState<{ dsp: string; arrangement: SidebarArrangement }>();
  const open = useRef(dsp);
  useLayoutEffect(() => {
    open.current = dsp;
  }, [dsp]);
  const saves = useRef<{ waiting?: NonNullable<typeof own>; busy: boolean }>({ busy: false });
  const save = async () => {
    const queue = saves.current;
    if (queue.busy) return;
    queue.busy = true;
    while (queue.waiting) {
      const { dsp: of, arrangement } = queue.waiting;
      queue.waiting = undefined;
      // The server keeps it for the DSP whose view is open: one left meanwhile isn't that one.
      if (of !== open.current) continue;
      await arrangeSidebar(arrangement).catch((error: unknown) => fail(messageOf(error)));
    }
    queue.busy = false;
  };
  const arrange = (arrangement: SidebarArrangement) => {
    if (!dsp) return;
    setOwn({ dsp, arrangement });
    saves.current.waiting = { dsp, arrangement };
    void save();
  };
  return {
    arrangement: (own && own.dsp === dsp ? own.arrangement : view?.sidebar) ?? unarranged,
    arrange,
  };
}
