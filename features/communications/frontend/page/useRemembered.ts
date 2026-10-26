import { useCallback, useState } from 'react';

// What someone chose for Communications' lists, as the workspace or the section they show,
// remembered in this browser for each member at each DSP, as the app's appearance is, so it's
// still chosen when they come back.

const key = (what: string, dsp: string, user: string) =>
  `dispatch-communications-${what}:${dsp}:${user}`;

function read(what: string, dsp: string, user: string) {
  try {
    return localStorage.getItem(key(what, dsp, user)) ?? undefined;
  } catch {
    return undefined;
  }
}

/** The `what` `user` last chose at `dsp`, if any, and choosing another, or none. */
export function useRemembered(what: string, dsp: string, user: string) {
  const [chosen, setChosen] = useState(() => read(what, dsp, user));
  const choose = useCallback(
    (value?: string) => {
      setChosen(value);
      try {
        if (value) localStorage.setItem(key(what, dsp, user), value);
        else localStorage.removeItem(key(what, dsp, user));
      } catch {
        // Not kept, it stays chosen while the page is open.
      }
    },
    [what, dsp, user],
  );
  return [chosen, choose] as const;
}
