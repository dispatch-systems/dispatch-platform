import { useState } from 'react';
import type { DspView, SidebarArrangement } from '../../../accounts/api/index.js';

const unarranged: SidebarArrangement = { order: [], hidden: [] };

/**
 * How the member arranged the sidebar of the DSP open: as its view says, or as they arranged it
 * since, which `arrange` shows at once.
 */
export function useArrangement(view: DspView | undefined) {
  const dsp = view?.dsp.id;
  const [own, setOwn] = useState<{ dsp: string; arrangement: SidebarArrangement }>();
  return {
    arrangement: (own && own.dsp === dsp ? own.arrangement : view?.sidebar) ?? unarranged,
    arrange: (arrangement: SidebarArrangement) => dsp && setOwn({ dsp, arrangement }),
  };
}
