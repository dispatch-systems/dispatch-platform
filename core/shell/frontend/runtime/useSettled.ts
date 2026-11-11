import { useEffect, useState } from 'react';

/** `value`, once it stops changing for a moment. */
export function useSettled(value: string, ms = 150) {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return settled;
}
