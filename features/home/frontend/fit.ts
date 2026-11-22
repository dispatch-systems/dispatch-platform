import { useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import { COLUMNS, GAP, ROWS } from './grid.js';

/**
 * The smallest the grid is laid out at, in pixels: as a 1440×900 window gives it, where every
 * widget fits each size it declares. Each cell is 79 by 83, with the gaps between them.
 */
export const SMALLEST = {
  width: COLUMNS * 79 + (COLUMNS - 1) * GAP,
  height: ROWS * 83 + (ROWS - 1) * GAP,
};

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
    const { width, height } = element.getBoundingClientRect();
    setSpace({ width, height });
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry!.contentRect;
      setSpace({ width, height });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const scale = space
    ? Math.min(1, space.width / SMALLEST.width, space.height / SMALLEST.height)
    : 1;
  const size = space && { width: space.width / scale, height: space.height / scale };
  const fit: CSSProperties | undefined =
    scale < 1 ? { flex: 'none', ...size, zoom: scale } : undefined;
  return { area, fit, size, scale };
}
