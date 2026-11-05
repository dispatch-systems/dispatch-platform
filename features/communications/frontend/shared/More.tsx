import { useEffect, useRef } from 'react';
import './more.css';

/**
 * Reads more as it nears view, as the end of a list scrolled to: again while it stays in view
 * after each read, until there's no more. Once a read `failed`, only as Try again is pressed.
 */
export function More({ read, failed = false }: { read: () => void; failed?: boolean }) {
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
  }, [read, failed]);
  if (failed) return <TryAgain retry={read} />;
  return <div ref={marker} className="more" aria-hidden="true" />;
}

/** Reads again what failed to read. */
export function TryAgain({ retry }: { retry: () => void }) {
  return (
    <button className="try-again" onClick={retry}>
      Try again
    </button>
  );
}
