import { useEffect, useRef, useState } from 'react';
import type { DspView } from '../../../accounts/api/index.js';
import type { ScheduledCollection, ScheduleCollection } from '../../api/index.js';
import { useCollectors } from '../../api/client.js';
import type { Leave } from '../../../shell/frontend/runtime/leave.js';
import { hashQuery, replaceHashQuery } from '../../../shell/frontend/runtime/navigation.js';
import { DataState } from '../../../shell/frontend/ui/index.js';
import { CollectionIcon } from './CollectionIcon.js';
import { CollectionSchedule } from './CollectionSchedule.js';
import { runShort } from './WeekPreview.js';
import { ruleText } from './rules.js';
import './collectors.css';

// Settings' Collectors page: every collection whose schedule the member sets, under the
// collector that reads it, each with its schedule in words and when it runs next; and the one
// chosen, `?collection=`, its schedule to set.

/** How a collection's last run ended, as its dot shows it. */
const state = (collection: ScheduledCollection) =>
  !collection.rules.length || !collection.enabled
    ? 'off'
    : collection.lastRun?.status === 'failed'
      ? 'failed'
      : 'on';

export function CollectorsTab({ view }: { view: DspView }) {
  const collectors = useCollectors(view.dsp.id);
  // What a change answered, shown until the collections are read again.
  const [answers, setAnswers] = useState<Partial<Record<ScheduleCollection, ScheduledCollection>>>(
    {},
  );
  const data = collectors.data;
  useEffect(() => setAnswers({}), [data]);
  const leaveRef = useRef<Leave | null>(null);
  const [chosen, setChosen] = useState(() => hashQuery().get('collection'));
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
        const choose = (collection: ScheduleCollection) => {
          const move = () => {
            setChosen(collection);
            replaceHashQuery({ ...Object.fromEntries(hashQuery()), collection });
          };
          if (leaveRef.current) leaveRef.current(move);
          else move();
        };
        return (
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
            />
          </div>
        );
      }}
    </DataState>
  );
}
