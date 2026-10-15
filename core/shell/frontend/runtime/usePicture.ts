import { useEffect, useRef, useState } from 'react';
import { picture } from './transfer.js';

/**
 * The picture at `url`, fetched once its box nears the screen, as an address the page shows:
 * an image's own address can't send the DSP's view. None until it comes, and none if it can't
 * be fetched; `lost` gives up on one the browser couldn't show.
 */
export function usePicture<T extends HTMLElement>(url: string | undefined) {
  const box = useRef<T>(null);
  const [shown, setShown] = useState<{ url: string; src?: string }>();
  useEffect(() => {
    const at = box.current;
    if (!url || !at) return;
    const stop = new AbortController();
    let src: string | undefined;
    const near = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        near.disconnect();
        picture(url, stop.signal).then(
          (blob) => {
            if (stop.signal.aborted) return;
            src = URL.createObjectURL(blob);
            setShown({ url, src });
          },
          () => {},
        );
      },
      { rootMargin: '200px' },
    );
    near.observe(at);
    return () => {
      near.disconnect();
      stop.abort();
      if (src) URL.revokeObjectURL(src);
    };
  }, [url]);
  const lost = () => {
    if (shown?.src) URL.revokeObjectURL(shown.src);
    setShown(shown && { url: shown.url });
  };
  return [box, shown && shown.url === url ? shown.src : undefined, lost] as const;
}
