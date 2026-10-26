import { CircleCheck, CircleX } from 'lucide-react';
import { errorLabel } from '../../shell/frontend/runtime/api.js';
import { useLiveReading } from '../api/client.js';
import { runDay, until } from './collectors/WeekPreview.js';
import './live-reading.css';

// How a live reader reads for the DSP, worded as the Collectors page words a collection's runs,
// for Settings to draw with the section that sets when the reader reads.

/** How often it's read again while shown, as a reader reads every few minutes. */
const POLL_MS = 60_000;

/**
 * The reader's last read, kept or failed, its next, and its last error, worded where the platform
 * words its code; nothing while it reads nothing for the DSP. `revision` is its section's: a save
 * of the section reads it again.
 */
export function LiveReading({
  reader,
  timezone,
  revision,
}: {
  reader: string;
  timezone: string;
  revision: number;
}) {
  const { data } = useLiveReading(reader, POLL_MS, String(revision));
  if (!data) return null;
  const { lastRead, nextRead, lastError } = data;
  return (
    <div className="live-reading">
      <dl>
        <dt>Last read</dt>
        <dd>
          {lastRead ? (
            <>
              {lastRead.kept ? (
                <CircleCheck size={15} className="live-reading-kept" aria-label="Kept" />
              ) : (
                <CircleX size={15} className="live-reading-failed" aria-label="Failed" />
              )}
              {runDay(lastRead.at, timezone)}
            </>
          ) : (
            'Never'
          )}
        </dd>
      </dl>
      <dl>
        <dt>Next read</dt>
        <dd>
          {runDay(nextRead, timezone)}
          <small>{until(nextRead)}</small>
        </dd>
      </dl>
      {lastError && (
        <dl>
          <dt>Last error</dt>
          <dd>
            {errorLabel(lastError.code) ?? lastError.code}
            <small>{runDay(lastError.at, timezone)}</small>
          </dd>
        </dl>
      )}
    </div>
  );
}
