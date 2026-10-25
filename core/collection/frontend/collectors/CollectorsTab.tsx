import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import type { DspView } from '../../../accounts/api/index.js';
import type { ScheduledCollection, ScheduleCollection } from '../../api/index.js';
import { useCollectors } from '../../api/client.js';
import type { Leave } from '../../../shell/frontend/runtime/leave.js';
import { hashQuery, replaceHashQuery } from '../../../shell/frontend/runtime/navigation.js';
import { DataState } from '../../../shell/frontend/ui/index.js';
import { useCollectionUpdates } from '../live-collection.js';
import { CollectedFields } from './CollectedFields.js';
import { CollectionIcon } from './CollectionIcon.js';
import { CollectionSchedule } from './CollectionSchedule.js';
import { runShort } from './WeekPreview.js';
import { ruleText } from './rules.js';
import './collectors.css';

// Settings' Collectors page: every collection whose schedule the member sets, under the
// collector that reads it, each with its schedule in words and when it runs next; and the one
// chosen, `?collection=`, its schedule to set, with the settings that go with it, then what it
// collects.

/** How often the collections are read again while one runs, for what it says it's doing. */
const POLL_MS = 3000;

/** How a collection's last run ended, as its dot shows it. */
const state = (collection: ScheduledCollection) =>
  !collection.rules.length || !collection.enabled
    ? 'off'
    : collection.lastRun?.status === 'failed'
      ? 'failed'
      : 'on';

export function CollectorsTab({
  view,
  collectionSections,
  pointed,
}: {
  view: DspView;
  /** The settings that go with a collection, as Settings draws them. */
  collectionSections?: (schedule: string) => ReactNode;
  /** The collection of a section the address points out, shown as it's pointed out. */
  pointed?: string;
}) {
  // It hears each collection end, as a rule's run ends, wherever it was started.
  useCollectionUpdates();
  const [watching, setWatching] = useState(false);
  const collectors = useCollectors(view.dsp.id, watching ? POLL_MS : 0);
  // What a change answered, shown until the collections are read again.
  const [answers, setAnswers] = useState<Partial<Record<ScheduleCollection, ScheduledCollection>>>(
    {},
  );
  const data = collectors.data;
  useEffect(() => setAnswers({}), [data]);
  const running = Boolean(data?.collections.some((each) => each.running));
  useEffect(() => setWatching(running), [running]);
  const leaveRef = useRef<Leave | null>(null);
  const [chosen, setChosen] = useState(() => hashQuery().get('collection'));
  const choose = useCallback((collection: string) => {
    const move = () => {
      setChosen(collection);
      replaceHashQuery({ ...Object.fromEntries(hashQuery()), collection });
    };
    if (leaveRef.current) leaveRef.current(move);
    else move();
  }, []);
  useEffect(() => {
    if (pointed) choose(pointed);
  }, [pointed, choose]);
  return (
    <DataState
      data={data}
      error={collectors.error}
      failed={!!collectors.error}
      retry={collectors.refresh}
    >
      {(data) => {
        const collections = data.collections.map((each) => answers[each.collection] ?? each);
        const current = collections.find((each) => each.collection === chosen) ?? collections[0];
        if (!current) return null;
        const groups = [...new Set(collections.map((each) => each.collectorLabel))];
        return (
          <>
            <div className="collectors">
              <nav className="collectors-list" aria-label="Collections">
                {groups.map((group, index) => (
                  <div className="collectors-group" key={group}>
                    <h3>
                      <span>
                        {group}
                        {!collections.some(
                          (each) => each.collectorLabel === group && each.connected,
                        ) && <em>Not connected</em>}
                      </span>
                      {index === 0 && <span>Next run</span>}
                    </h3>
                    {collections
                      .filter((each) => each.collectorLabel === group)
                      .map((each) => (
                        <button
                          type="button"
                          key={each.collection}
                          className="collectors-item"
                          aria-current={each === current || undefined}
                          onClick={() => choose(each.collection)}
                        >
                          <span className="collectors-tile">
                            <CollectionIcon feature={each.feature} size={14} />
                          </span>
                          <span className="collectors-name">
                            <span>
                              {each.label}
                              <i className={`dot run-${state(each)}`} />
                            </span>
                            <small>
                              {each.rules.length
                                ? each.rules.map((rule) => ruleText(rule)).join('; ')
                                : 'Not scheduled'}
                            </small>
                          </span>
                          <span className="collectors-next">
                            {each.enabled && each.nextRun
                              ? runShort(each.nextRun, data.timezone)
                              : each.rules.length
                                ? 'Off'
                                : ''}
                          </span>
                        </button>
                      ))}
                  </div>
                ))}
              </nav>
              <CollectionSchedule
                key={current.collection}
                collection={current}
                timezone={data.timezone}
                leaveRef={leaveRef}
                saved={(answer) => {
                  setAnswers((before) => ({ ...before, [answer.collection]: answer }));
                  collectors.refresh();
                }}
                refresh={collectors.refresh}
                settings={collectionSections?.(current.collection)}
              />
            </div>
            <CollectedFields collection={current} />
          </>
        );
      }}
    </DataState>
  );
}
