import { useCallback, useEffect, useState } from 'react';
import type { DvicInspection, DvicInspections } from '../../../../shared/contracts/dvic.js';
import { api } from '../../app/api.js';
import { readInspectionWeek } from '../../lib/dvic.js';
import { messageOf } from '../../lib/errors.js';
import { performancePolicy } from '../../lib/performance-policy.js';

export function useInspections(from: string, to: string, publication: string) {
  const [result, setResult] = useState<{ from: string; rows: DvicInspection[] }>();
  const [failure, setFailure] = useState<{ from: string; message: string }>();
  const [revision, setRevision] = useState(0);
  const refresh = useCallback(() => setRevision((n) => n + 1), []);
  useEffect(() => {
    const controller = new AbortController();
    let reading = false;
    async function read() {
      if (reading || document.hidden || !navigator.onLine) return;
      reading = true;
      try {
        const rows = await readInspectionWeek((after) => {
          const query = new URLSearchParams({
            from,
            to,
            limit: '500',
            ...(after ? { after } : {}),
          });
          return api<DvicInspections>(
            '/api/dsp/dvic/inspections?' + query,
            undefined,
            controller.signal,
          );
        }, controller.signal);
        if (!controller.signal.aborted) {
          setResult({ from, rows });
          setFailure(undefined);
        }
      } catch (cause) {
        if (!controller.signal.aborted) setFailure({ from, message: messageOf(cause) });
      } finally {
        reading = false;
      }
    }
    void read();
    const timer = setInterval(() => void read(), performancePolicy.recoveryPollMs);
    const visible = () => {
      if (!document.hidden) void read();
    };
    document.addEventListener('visibilitychange', visible);
    window.addEventListener('online', visible);
    return () => {
      controller.abort();
      clearInterval(timer);
      document.removeEventListener('visibilitychange', visible);
      window.removeEventListener('online', visible);
    };
  }, [from, to, publication, revision]);
  return {
    rows: result?.from === from ? result.rows : undefined,
    error: failure?.from === from ? failure.message : '',
    refresh,
  };
}
