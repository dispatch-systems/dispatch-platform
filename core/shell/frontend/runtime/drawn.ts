import { useLayoutEffect } from 'react';

/**
 * Marks the page the app has drawn on the document's root, as `data-app-ready="<scope>:<page>"`
 * (`dsp:overview`, `platform:dsps`), for what waits on the app, as a browser test does: none
 * while a page's code or the DSP it is in still loads, or once it failed to. Rendered beside the
 * page, inside the boundary it loads under, so it is drawn with the page and hidden with it. No
 * one sees it.
 */
export function Drawn({ page }: { page: string }) {
  useLayoutEffect(() => {
    const root = document.documentElement;
    root.dataset.appReady = page;
    return () => {
      delete root.dataset.appReady;
    };
  }, [page]);
  return null;
}
