import { Fragment, useContext } from 'react';
import { CircleAlert, RotateCw, Users } from 'lucide-react';
import type { Conversation, Message, Workday } from '../../api/index.js';
import { clock, dayLabel, dayOf, textStatus, whyNotSent, workdayLabel } from '../shared/format.js';
import { paused, runs } from './messages.js';
import { Shown } from '../display/display.js';
import { Face, SenderFace } from '../shared/Face.js';
import { KINDS } from './kinds.js';
import { PartyFace } from './party.js';
import { MessageBody } from '../shared/MessageBody.js';
import {
  FaceButton,
  MessageMeta,
  MessageRun,
  sameSender,
  SenderLine,
} from '../shared/MessageRun.js';
import { DayLabel, PauseLabel, ThreadFrame } from '../shared/ThreadFrame.js';
import type { OpensProfile } from '../shared/usePanel.js';
import { Switched, WorkdayLine } from '../todays-routes/Workday.js';
import './thread.css';

// A conversation's messages, in the order written, under the day each was written: the
// member's on the left beside their photo, which opens their profile, the DSP's on the right
// under the name and photo of whoever sent them, named once another sent the DSP's before them
// that day, or the phone's own, and the group one went to, which opens it. Each run's time is
// under it, or each message's, or above the first after a pause, as the member chose (`Shown`);
// plain, every run is under its writer's name and time instead. A text the DSP sent says how it
// went under it, the run's last, or one that failed, with why where that's known, and Retry. The
// thread goes to its latest as `latest` changes. A driver's work conversation begins each workday with a line across it, with
// the route and van they drove and the work phone they texted on, earlier days dimmed, and marks
// where they texted from another work phone that day.

export function Thread({
  conversation,
  messages,
  more,
  readOlder,
  olderFailed,
  latest,
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
  /** Whether the latest read of older ones failed. */
  olderFailed: boolean;
  /** Goes to the latest message as it changes. */
  latest?: number;
  timeZone: string;
  now: number;
  /** What makes their photo by their messages open their profile, where it does. */
  profile?: Partial<OpensProfile>;
  /** Opens the group a message went to. */
  openGroup: (id: string) => void;
  /** Sends again a text the DSP sent that failed. */
  retry?: (message: Message) => void;
  /** For a driver's work conversation, each workday's route and van. */
  workdays?: readonly Workday[];
}) {
  const person = conversation.with;
  const { address } = KINDS[conversation.kind].attachments;
  const { style, times } = useContext(Shown);
  const plain = style === 'plain';
  const pauses = !plain && times === 'after_pause';
  const today = dayOf(now, timeZone);
  return (
    <ThreadFrame
      items={messages}
      latest={latest}
      more={more}
      readOlder={readOlder}
      olderFailed={olderFailed}
    >
      {runs(messages, timeZone).map(({ day, at, runs }) => (
        <div key={day} className={workdays && day !== today ? 'day-messages past' : 'day-messages'}>
          {workdays ? (
            <WorkdayLine
              label={workdayLabel(day, timeZone, now)}
              workday={workdays.find((each) => each.day === day)}
              phone={runs[0]?.messages[0]?.number ?? null}
              today={day === today}
            />
          ) : (
            <DayLabel>{dayLabel(at, timeZone, now)}</DayLabel>
          )}
          {runs.map(({ key, messages: written }, at) => {
            const first = written[0]!;
            const sender = first.sender;
            const previous = runs[at - 1]?.messages;
            const before = previous?.[0]?.number;
            const switched = workdays && before && first.number && first.number !== before;
            // The DSP's sender is named once another sent the DSP's before them that day.
            const sentBefore = runs.slice(0, at).findLast((each) => each.messages[0]?.sender)
              ?.messages[0]?.sender;
            const named = plain || (sender && (!sameSender(sentBefore, sender) || first.group));
            return (
              <Fragment key={key}>
                {switched && <Switched number={first.number!} />}
                {pauses && paused(previous?.at(-1)?.at, first.at) && (
                  <PauseLabel>{clock(first.at, timeZone)}</PauseLabel>
                )}
                <MessageRun
                  direction={first.direction}
                  face={
                    sender ? (
                      <SenderFace sender={sender} size="message" />
                    ) : profile ? (
                      // Who it's with, above, opens it from the keyboard.
                      <FaceButton label={person.name} {...profile}>
                        <Face {...person} size="message" />
                      </FaceButton>
                    ) : (
                      <PartyFace conversation={conversation} size="message" />
                    )
                  }
                  sender={
                    named && (
                      <SenderLine
                        name={sender?.name ?? person.name}
                        at={plain && clock(first.at, timeZone)}
                      >
                        {first.group && (
                          <button className="to-group" onClick={() => openGroup(first.group!.id)}>
                            <Users size={11} aria-hidden="true" />
                            {first.group.name}
                          </button>
                        )}
                      </SenderLine>
                    )
                  }
                >
                  {written.map((message, at) => {
                    const last = at === written.length - 1;
                    return (
                      <Fragment key={message.id}>
                        <MessageBody message={message} address={address} />
                        <Meta
                          message={message}
                          time={!plain && !pauses && (last || times === 'each_message')}
                          status={last || message.status === 'failed'}
                          timeZone={timeZone}
                          retry={retry}
                        />
                      </Fragment>
                    );
                  })}
                </MessageRun>
              </Fragment>
            );
          })}
        </div>
      ))}
    </ThreadFrame>
  );
}

/**
 * Under a message: when it was written, while `time` says to show it, and, while `status` says
 * to, how a text the DSP sent went: Not sent, with why where that's known, and Retry, once it
 * failed.
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
  const why = whyNotSent(message.error);
  return (
    <MessageMeta time={time && clock(message.at, timeZone)}>
      {gone === 'failed' ? (
        <>
          <span className="failed">
            <CircleAlert size={12} aria-hidden="true" />
            {textStatus.failed}
          </span>
          {why && <span className="why">{` · ${why}`}</span>}
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
    </MessageMeta>
  );
}
