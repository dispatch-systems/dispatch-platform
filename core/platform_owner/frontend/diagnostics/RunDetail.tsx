import { useState } from 'react';
import type { Job, JobMetrics } from '../../../collection/api/index.js';
import { errorLabel } from '../../../shell/frontend/runtime/api.js';
import { runWording } from '../../../shell/frontend/runtime/slots.js';
import { DetailList } from '../../../shell/frontend/ui/index.js';
import { bytes, duration, title } from '../../../shell/frontend/lib/format.js';

export const memory = (value: number | null) =>
  value === null ? 'Not sampled' : bytes(value, 'MiB', 1);

const phases: [label: string, tone: string, read: (attempt: JobMetrics) => number | null][] = [
  ['Queue wait', 'phase-queue', (attempt) => attempt.queueMs],
  ['Browser & sign-in', 'phase-sign-in', (attempt) => attempt.authenticationMs],
  ['Human verification', 'phase-verification', (attempt) => attempt.verificationMs],
  ['Collection', 'phase-collection', (attempt) => attempt.collectionMs],
  ['Publishing', 'phase-publishing', (attempt) => attempt.publicationMs],
];

/** One collection's attempts: where the time went, memory, records and page reads. */
export function RunDetail({ job }: { job: Job }) {
  const metrics = job.metrics ?? [];
  const [chosen, setChosen] = useState<number>();
  const attempt = metrics.find((m) => m.attempt === chosen) ?? metrics.at(-1);
  if (!attempt) return <p className="muted">Performance was not recorded for this collection.</p>;
  const reads = attempt.pageReads;
  // What it collected and its page reads, as an owner words them; Diagnostics loads with them.
  const wording = runWording();
  const SlowReads = wording?.slowReads;
  return (
    <div className="run-detail">
      {metrics.length > 1 && (
        <div className="diagnostics-chips" role="group" aria-label="Attempts">
          {metrics.map((m) => (
            <button
              key={m.attempt}
              aria-pressed={m === attempt}
              onClick={() => setChosen(m.attempt)}
            >
              Attempt {m.attempt} · {title(m.outcome)}
            </button>
          ))}
        </div>
      )}
      <section aria-label={`Attempt ${attempt.attempt}`}>
        {attempt.error && (
          <p className="run-error">
            {errorLabel(attempt.error) ?? title(attempt.error)}
            {attempt.detail && ` · ${title(attempt.detail)}`}
          </p>
        )}
        <h3>Where the time went</h3>
        <div className="run-phases" aria-hidden="true">
          {phases.map(([label, tone, read]) =>
            read(attempt) ? (
              <span key={label} className={tone} style={{ flexGrow: read(attempt)! }} />
            ) : null,
          )}
        </div>
        <dl className="run-phase-legend">
          {phases.map(([label, tone, read]) => (
            <div key={label}>
              <dt>
                <i className={tone} />
                {label}
              </dt>
              <dd>{duration(read(attempt))}</dd>
            </div>
          ))}
        </dl>
        {attempt.outcome === 'interrupted' && (
          <p className="muted">Timings stop at the last saved measurement before interruption.</p>
        )}
        <h3>Memory and records</h3>
        <DetailList
          className="run-facts"
          items={[
            ['Elapsed', duration(attempt.elapsedMs)],
            ['Peak private memory', memory(attempt.peakPrivateBytes)],
            ['Peak summed memory (RSS)', memory(attempt.peakRssBytes)],
            ['Collected', wording?.collected(attempt) ?? '—'],
            ...(reads && wording ? wording.reads(reads) : []),
            [
              'Memory samples',
              `${attempt.memorySamples} · ${attempt.incompleteMemorySamples} incomplete`,
            ],
          ]}
        />
        {SlowReads && <SlowReads attempt={attempt} />}
      </section>
    </div>
  );
}
