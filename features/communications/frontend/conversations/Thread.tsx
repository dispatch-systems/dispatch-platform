import { Fragment, useContext, useLayoutEffect, useRef } from 'react';
import { CircleAlert, RotateCw, Users } from 'lucide-react';
import type { Conversation, Message, Workday, Writing } from '../../api/index.js';
import { clock, dayLabel, dayOf, textStatus, workdayLabel } from '../shared/format.js';
import { runs } from './messages.js';
import { Shown } from '../display/display.js';
import { Face, Party, SenderFace } from '../shared/Face.js';
import { MessageBody } from '../shared/MessageBody.js';
import { More } from '../shared/More.js';
import type { OpensProfile } from '../shared/usePanel.js';
import { Switched, WorkdayLine } from '../todays-routes/Workday.js';

// A conversation's messages, in the order written, under the day each was written: the
// member's on the left beside their photo, which opens their profile, the DSP's on the right
// under the name and photo of whoever sent them, or the phone's own, and the group one went to,
// which opens it; and below, those at the DSP writing in it now. Each run's time is under it, or
// each message's, as the member chose (`Shown`); plain, every run is under its writer's name and
// time instead. A text the DSP sent says how it went under it, the run's last, or one that
// failed, with Retry. A driver's work conversation begins each workday with a line across it, with
// the route and van they drove and the work phone they texted on, earlier days dimmed, and marks
// where they texted from another work phone that day.

export function Thread({
  conversation,
  messages,
  more,
  readOlder,
  writing,
  timeZone,
  now,
  profile,
  openGroup,
  retry,
  workdays,
}: {
  conversation: Conversation;
  messages: readonly Message[];
  more: boolean;
  readOlder: () => void;
  writing: readonly Writing[];
  timeZone: string;
  now: number;
  /** What makes the member's photo open their profile. */
  profile: Partial<OpensProfile>;
  /** Opens the group a message went to. */
  openGroup: (id: string) => void;
  /** Sends again a text the DSP sent that failed. */
  retry?: (message: Message) => void;
  /** For a driver's work conversation, each workday's route and van. */
  workdays?: readonly Workday[];
}) {
  const scroller = useRef<HTMLDivElement>(null);
  // Where it was before the latest change: at the latest message, or how far up, which older
  // messages coming above keep.
  const before = useRef({ first: '', height: 0, top: 0, latest: true });
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const first = messages[0]?.id ?? '';
    const was = before.current;
    if (was.first && first !== was.first && !was.latest)
      element.scrollTop = was.top + element.scrollHeight - was.height;
    else if (was.latest) element.scrollTop = element.scrollHeight;
    before.current = { ...was, first, height: element.scrollHeight, top: element.scrollTop };
  }, [messages, writing]);
  const scrolled = () => {
    const element = scroller.current;
    if (!element) return;
    before.current = {
      ...before.current,
      height: element.scrollHeight,
      top: element.scrollTop,
      latest: element.scrollHeight - element.scrollTop - element.clientHeight < 80,
    };
  };
  const person = conversation.with;
  const texted = conversation.kind !== 'slack';
  const { style, times } = useContext(Shown);
  const plain = style === 'plain';
  const today = dayOf(now, timeZone);
  return (
    <div className="thread" ref={scroller} onScroll={scrolled}>
      {more && <More read={readOlder} />}
      <div className="thread-messages">
        {runs(messages, timeZone).map(({ day, at, runs }) => (
          <div
            key={day}
            className={workdays && day !== today ? 'day-messages past' : 'day-messages'}
          >
            {workdays ? (
              <WorkdayLine
                label={workdayLabel(day, timeZone, now)}
                workday={workdays.find((each) => each.day === day)}
                phone={runs[0]?.messages[0]?.number ?? null}
                today={day === today}
              />
            ) : (
              <div className="day">{dayLabel(at, timeZone, now)}</div>
            )}
            {runs.map(({ key, messages: written }, at) => {
              const first = written[0]!;
              const sender = first.sender;
              const before = runs[at - 1]?.messages[0]?.number;
              const switched = workdays && before && first.number && first.number !== before;
              return (
                <Fragment key={key}>
                  {switched && <Switched number={first.number!} />}
                  <div className={`msg-group ${first.direction}`}>
                    {sender ? (
                      <SenderFace sender={sender} size="message" />
                    ) : texted ? (
                      <Party conversation={conversation} size="message" />
                    ) : (
                      // Left out of Tab's order: who it's with, above, opens it from the keyboard.
                      <button
                        className="face-button"
                        tabIndex={-1}
                        aria-label={person.name}
                        {...profile}
                      >
                        <Face {...person} size="message" />
                      </button>
                    )}
                    <div className="stack">
                      {(sender || plain) && (
                        <div className="sender">
                          <span className="sender-name">{sender?.name ?? person.name}</span>
                          {plain && <span className="sent-at">{clock(first.at, timeZone)}</span>}
                          {first.group && (
                            <button className="to-group" onClick={() => openGroup(first.group!.id)}>
                              <Users size={11} aria-hidden="true" />
                              {first.group.name}
                            </button>
                          )}
                        </div>
                      )}
                      {written.map((message, at) => {
                        const last = at === written.length - 1;
                        return (
                          <Fragment key={message.id}>
                            <MessageBody message={message} texted={texted} />
                            <Meta
                              message={message}
                              time={!plain && (last || times === 'each_message')}
                              status={last || message.status === 'failed'}
                              timeZone={timeZone}
                              retry={retry}
                            />
                          </Fragment>
                        );
                      })}
                    </div>
                  </div>
                </Fragment>
              );
            })}
          </div>
        ))}
        {writing.map((writer) => (
          <div key={writer.userId} className="msg-group out">
            <Face name={writer.name} code={writer.code} photo={writer.photo} size="message" />
            <div className="stack">
              <div className="sender">
                <span className="sender-name">{writer.name}</span>
              </div>
              <div className="typing" role="status" aria-label={`${writer.firstName} is typing`}>
                <i />
                <i />
                <i />
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

/**
 * Under a message: when it was written, while `time` says to show it, and, while `status` says
 * to, how a text the DSP sent went: Not sent, with Retry, once it failed.
 */
function Meta({
  message,
  time,
  status,
  timeZone,
  retry,
}: {
  message: Message;
  time: boolean;
  status: boolean;
  timeZone: string;
  retry?: (message: Message) => void;
}) {
  const gone = status ? message.status : null;
  if (!time && !gone) return null;
  return (
    <div className="meta">
      {time && (
        <span className="at">
          {clock(message.at, timeZone)}
          {gone && ' · '}
        </span>
      )}
      {gone === 'failed' ? (
        <>
          <span className="failed">
            <CircleAlert size={12} aria-hidden="true" />
            {textStatus.failed}
          </span>
          {retry && (
            <>
              {' · '}
              <button className="retry" onClick={() => retry(message)}>
                <RotateCw size={11} aria-hidden="true" />
                Retry
              </button>
            </>
          )}
        </>
      ) : (
        gone && (
          <span className={gone === 'waiting' ? 'sending' : undefined}>{textStatus[gone]}</span>
        )
      )}
    </div>
  );
}
