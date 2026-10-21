import { useCallback, useState } from 'react';

// The workspace someone chose for Communications' lists, remembered in this browser for each
// member at each DSP, as the app's appearance is, so it's still chosen when they come back.

const key = (dsp: string, user: string) => `dispatch-communications-workspace:${dsp}:${user}`;

function read(dsp: string, user: string) {
  try {
    return localStorage.getItem(key(dsp, user)) ?? undefined;
  } catch {
    return undefined;
  }
}

/** The workspace `user` last chose at `dsp`, if any, and choosing another; none for every one. */
export function useWorkspaceChoice(dsp: string, user: string) {
  const [chosen, setChosen] = useState(() => read(dsp, user));
  const choose = useCallback(
    (team?: string) => {
      setChosen(team);
      try {
        if (team) localStorage.setItem(key(dsp, user), team);
        else localStorage.removeItem(key(dsp, user));
      } catch {
        // Not kept, it stays chosen while the page is open.
      }
    },
    [dsp, user],
  );
  return [chosen, choose] as const;
}
