// Which of the member's tabs at the DSP alerts them: one at a time, the one they last had in
// front of them. Each holds a place in line for the lock `name` (Web Locks); the tab focused
// takes the lock from the one holding it (`steal`), and one closing hands it to the next in
// line. A browser without Web Locks alerts from each tab, and what they saw, shared, keeps one
// message from alerting twice.

/**
 * Has this tab take its turn alerting under `name`, `leading` told as it starts and stops, until
 * the answer is called.
 */
export function lead(name: string, leading: (leads: boolean) => void) {
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
