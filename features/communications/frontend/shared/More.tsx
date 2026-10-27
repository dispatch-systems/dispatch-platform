import { useEffect, useRef } from 'react';
import './more.css';

/**
 * Reads more as it nears view, as the end of a list scrolled to: again while it stays in view
 * after each read, until there's no more.
 */
export function More({ read }: { read: () => void }) {
  const marker = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = marker.current;
    if (!element) return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry?.isIntersecting) read();
      },
      { rootMargin: '240px' },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [read]);
  return <div ref={marker} className="more" aria-hidden="true" />;
}
