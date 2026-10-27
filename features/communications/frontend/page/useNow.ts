import { useEffect, useState } from 'react';

/** How often the times waited move on. */
const TICK_MS = 30_000;

/** The time now, moving on every `TICK_MS`, for the times the page shows. */
export function useNow() {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), TICK_MS);
    return () => clearInterval(timer);
  }, []);
  return now;
}
