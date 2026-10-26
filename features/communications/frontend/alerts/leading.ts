// Which of the member's tabs at the DSP alerts them: one at a time, the one they last had in
// front of them. Each holds a place in line for the lock `name` (Web Locks); the tab focused
// takes the lock from the one holding it (`steal`), and one closing hands it to the next in
// line. A browser without Web Locks alerts from each tab, and what they saw, shared, keeps one
// message from alerting twice.

/**
 * Has this tab take its turns alerting under `name`, each told to `turn` as it starts, with a
 * signal aborted as it ends, until the answer is called.
 */
export function lead(name: string, turn: (ends: AbortSignal) => void) {
  let current: AbortController | undefined;
  const leading = (leads: boolean) => {
    current?.abort();
    current = leads ? new AbortController() : undefined;
    if (current) turn(current.signal);
  };
  if (!('locks' in navigator)) {
    leading(true);
    return () => leading(false);
  }
  let closed = false;
  let leads = false;
  let taking = false;
  let line: AbortController | undefined;
  let release = () => {};
  /** Asks for the lock: to take it now, or to wait in line for it. */
  const ask = (take: boolean) => {
    line?.abort();
    line = take ? undefined : new AbortController();
    taking = take;
    let granted = false;
    navigator.locks
      .request(name, line ? { signal: line.signal } : { steal: true }, () => {
        // Taken as the tab closed, it's let go at once.
        if (closed) return;
        granted = true;
        leads = true;
        taking = false;
        leading(true);
        return new Promise<void>((done) => (release = done));
      })
      .catch(() => {
        if (take) taking = false;
        // Taken by a tab focused since: back in line, unless closing.
        if (!granted || closed) return;
        leads = false;
        leading(false);
        ask(false);
      });
  };
  const focused = () => {
    if (!leads && !taking && document.hasFocus()) ask(true);
  };
  addEventListener('focus', focused);
  document.addEventListener('visibilitychange', focused);
  ask(document.hasFocus());
  return () => {
    closed = true;
    removeEventListener('focus', focused);
    document.removeEventListener('visibilitychange', focused);
    line?.abort();
    release();
    if (leads) leading(false);
  };
}
