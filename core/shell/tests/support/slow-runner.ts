import type { Page } from '@playwright/test';

/**
 * A busy CI runner on this machine, to prove a timing fix with `dispatchdev prove --cpu` or
 * `--late-frames`; off unless asked for. DISPATCH_CPU_THROTTLE slows the page's CPU that many
 * times. DISPATCH_LATE_FRAMES runs each animation frame callback that many milliseconds late,
 * as on a runner that draws late, so a test's calls can land between a change and the frame
 * that follows it.
 */
export async function slowRunner(page: Page, env: NodeJS.ProcessEnv = process.env) {
  const rate = Number(env.DISPATCH_CPU_THROTTLE ?? 1);
  if (rate > 1) {
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Emulation.setCPUThrottlingRate', { rate });
  }
  const late = Number(env.DISPATCH_LATE_FRAMES ?? 0);
  if (late > 0) await page.addInitScript(lateFrames, late);
}

/** Runs in the page: each callback waits `ms`, then for the next frame, in the order asked. */
function lateFrames(ms: number) {
  const request = window.requestAnimationFrame.bind(window);
  const cancel = window.cancelAnimationFrame.bind(window);
  const pending = new Map<number, { timer: number; frame?: number }>();
  let next = 1;
  window.requestAnimationFrame = (callback) => {
    const id = next++;
    const entry: { timer: number; frame?: number } = {
      timer: window.setTimeout(() => {
        entry.frame = request((time) => {
          pending.delete(id);
          callback(time);
        });
      }, ms),
    };
    pending.set(id, entry);
    return id;
  };
  window.cancelAnimationFrame = (id) => {
    const entry = pending.get(id);
    if (!entry) return;
    clearTimeout(entry.timer);
    if (entry.frame) cancel(entry.frame);
    pending.delete(id);
  };
}
