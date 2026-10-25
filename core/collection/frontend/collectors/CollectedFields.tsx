import { Fragment, useEffect, useState } from 'react';
import { ChevronRight, EyeOff, Lock } from 'lucide-react';
import type { RunCounts, ScheduledCollection } from '../../api/index.js';
import {
  collectedOf,
  type Collected,
  type CollectedRecords,
} from '../../../shell/frontend/runtime/slots.js';

// What the collection chosen on the Collectors page collects, as the feature that keeps it says:
// each kind of record it keeps, its fields each with a made-up value, and how many of them its
// latest successful run brought; beside them what a run reads and does, what it skips, what it
// reads but doesn't keep, and what it keeps of someone's own.

export function CollectedFields({ collection }: { collection: ScheduledCollection }) {
  const { feature, collection: name } = collection;
  const [shown, setShown] = useState<{ of: string; collected?: Collected }>();
  useEffect(() => {
    let current = true;
    collectedOf(feature, name)
      .then((collected) => {
        if (current) setShown({ of: name, collected });
      })
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, [feature, name]);
  const collected = shown?.of === name ? shown.collected : undefined;
  if (!collected) return null;
  const personal = collected.records.flatMap((records) =>
    records.fields.flatMap(({ label, personal }) =>
      personal ? [personal === true ? label : personal] : [],
    ),
  );
  const places = [collection.collectorLabel, ...collected.from];
  return (
    <section className="collected" aria-label="What’s collected">
      <header className="collected-head">
        <h2>What’s collected</h2>
        <span className="collected-from">
          {places.map((place, index) => (
            <Fragment key={index}>
              {index > 0 && <ChevronRight size={13} aria-hidden="true" />}
              {index === places.length - 1 ? <b>{place}</b> : place}
            </Fragment>
          ))}
        </span>
      </header>
      <div className="collected-body">
        <div className={`collected-records${collected.records.length === 1 ? ' is-single' : ''}`}>
          {collected.records.map((records) => (
            <Records
              key={records.label}
              records={records}
              datasets={collected.datasets}
              counts={collection.counts}
            />
          ))}
        </div>
        <dl className="collected-about">
          <div>
            <dt>{collection.choice?.label ?? 'Period'}</dt>
            {collected.period.map(({ label, detail }) => (
              <dd key={label}>
                {label}
                {detail && <small>{detail}</small>}
              </dd>
            ))}
          </div>
          <div>
            <dt>Each run</dt>
            <dd>{collected.each}</dd>
          </div>
          {collected.skips && (
            <div>
              <dt>Skips</dt>
              {collected.skips.map((skip) => (
                <dd key={skip}>{skip}</dd>
              ))}
            </div>
          )}
          {collected.dropped && (
            <div>
              <dt>Not kept</dt>
              <dd className="collected-chips">
                {collected.dropped.map((dropped) => (
                  <span className="collected-chip is-dropped" key={dropped}>
                    <EyeOff size={13} aria-hidden="true" />
                    {dropped}
                  </span>
                ))}
              </dd>
            </div>
          )}
          {personal.length > 0 && (
            <div>
              <dt>Personal</dt>
              <dd className="collected-chips">
                {personal.map((field) => (
                  <span className="collected-chip is-personal" key={field}>
                    <Lock size={12} aria-hidden="true" />
                    {field}
                  </span>
                ))}
              </dd>
            </div>
          )}
        </dl>
      </div>
    </section>
  );
}

/** One kind of record and its fields, with how many the latest successful run brought. */
function Records({
  records: { label, icon: Icon, counted, fields },
  datasets,
  counts,
}: {
  records: CollectedRecords;
  datasets?: boolean;
  counts: RunCounts | null;
}) {
  const count = counted ? counts?.[counted] : undefined;
  return (
    <section className="collected-group">
      <header>
        <span className="collected-tile">
          <Icon size={15} aria-hidden="true" />
        </span>
        <h3>{label}</h3>
        <span>
          {datasets
            ? `${fields.length} dataset${fields.length === 1 ? '' : 's'}`
            : count?.toLocaleString('en-US')}
        </span>
      </header>
      <ul>
        {fields.map(({ label, icon: FieldIcon, example, personal }) => (
          <li key={label}>
            <FieldIcon size={15} aria-hidden="true" />
            <span>{label}</span>
            {personal && <Lock className="collected-personal" size={12} aria-label="Personal" />}
            <em>{example}</em>
          </li>
        ))}
      </ul>
    </section>
  );
}
