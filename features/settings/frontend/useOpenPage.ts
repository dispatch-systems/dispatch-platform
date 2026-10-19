import { useEffect, useState, useTransition } from 'react';
import type { DspView } from '../../../core/accounts/api/index.js';
import type { SettingsAnswer } from '../../../core/settings/api/index.js';
import { holdAddress } from '../../../core/shell/frontend/runtime/holds.js';
import { addressedPage, followAddress, leavePage } from './address.js';
import { drawnPages, loadPage, openPage, prefetchPage } from './pages.js';

/**
 * Which of Settings' pages is open, as the address names it: the pages there are, and the one
 * open, which follows the address as it moves, once the next page's code and reads are in.
 * `pending` until they are. The address moves, by Back, the browser's Back or Forward, a link or
 * a hash typed, and the page is drawn anew, as for another role viewed as, only once its editors
 * let it, as its own moves do.
 */
export function useOpenPage(settings: SettingsAnswer, view: DspView) {
  const [requested, setRequested] = useState(addressedPage);
  const [pending, startTransition] = useTransition();
  const page = openPage(settings, view, requested);
  useEffect(
    () =>
      followAddress(() => {
        const page = addressedPage();
        if (page) {
          void loadPage(settings, view, page).catch(() => undefined);
          prefetchPage(page, view, true);
        }
        startTransition(() => setRequested(page));
      }),
    [settings, view],
  );
  useEffect(() => holdAddress(leavePage), []);
  return { pages: drawnPages(settings, view), page, pending };
}
