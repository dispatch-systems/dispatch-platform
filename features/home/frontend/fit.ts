import { useLayoutEffect, useRef, useState, type CSSProperties } from 'react';

/**
 * The smallest the grid is laid out at, in pixels: as a 1440×900 window gives it, where every
 * widget fits each size it declares.
 */
export const SMALLEST = { width: 1128, height: 677 };

/**
 * The grid filling the space the window gives it: where that's smaller than `SMALLEST`, the grid
 * is laid out larger, as much as it needs to be, and scaled evenly back down to fill it, so every
 * widget keeps the room it fits in and the page never scrolls. `area` is the space, `fit` the
 * grid's style in it, `size` the grid's own size before it's scaled, and `scale` how much it's
 * scaled: a pointer moving a pixel across the window moves `1 / scale` of the grid's own.
 */
export function useFit() {
  const area = useRef<HTMLDivElement>(null);
  const [space, setSpace] = useState<{ width: number; height: number }>();
  // Measured before it's first shown, so it never shows at another size first.
  useLayoutEffect(() => {
    const element = area.current;
    if (!element) return;
    // A space not laid out yet has no size to fit to.
    const measured = ({ width, height }: { width: number; height: number }) => {
      if (width && height) setSpace({ width, height });
    };
    measured(element.getBoundingClientRect());
    const observer = new ResizeObserver(([entry]) => measured(entry!.contentRect));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const scale = space
    ? Math.min(1, space.width / SMALLEST.width, space.height / SMALLEST.height)
    : 1;
  const size = space && { width: space.width / scale, height: space.height / scale };
  // Scaled, it lies over the space, which the page alone sizes: its own size never holds the
  // space open as the window narrows.
  const fit: CSSProperties | undefined =
    scale < 1 ? { position: 'absolute', inset: '0 auto auto 0', ...size, zoom: scale } : undefined;
  return { area, fit, size, scale };
}
