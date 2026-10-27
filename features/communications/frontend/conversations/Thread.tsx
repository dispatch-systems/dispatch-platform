import { Fragment, useContext } from 'react';
import { CircleAlert, RotateCw, Users } from 'lucide-react';
import type { Conversation, Message, Workday, Writing } from '../../api/index.js';
import { clock, dayLabel, dayOf, textStatus, workdayLabel } from '../shared/format.js';
import { runs } from './messages.js';
import { Shown } from '../display/display.js';
import { Face, Party, SenderFace } from '../shared/Face.js';
import { MessageBody } from '../shared/MessageBody.js';
import { FaceButton, MessageMeta, MessageRun, SenderLine } from '../shared/MessageRun.js';
import { DayLabel, ThreadFrame } from '../shared/ThreadFrame.js';
import type { OpensProfile } from '../shared/usePanel.js';
import { Switched, WorkdayLine } from '../todays-routes/Workday.js';
import './thread.css';

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
  const person = conversation.with;
  const texted = conversation.kind !== 'slack';
  const { style, times } = useContext(Shown);
  const plain = style === 'plain';
  const today = dayOf(now, timeZone);
  return (
    <ThreadFrame items={messages} after={writing} more={more} readOlder={readOlder}>
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
            const before = runs[at - 1]?.messages[0]?.number;
            const switched = workdays && before && first.number && first.number !== before;
            return (
              <Fragment key={key}>
                {switched && <Switched number={first.number!} />}
                <MessageRun
                  direction={first.direction}
                  face={
                    sender ? (
                      <SenderFace sender={sender} size="message" />
                    ) : texted ? (
                      <Party conversation={conversation} size="message" />
                    ) : (
                      // Who it's with, above, opens it from the keyboard.
                      <FaceButton label={person.name} {...profile}>
                        <Face {...person} size="message" />
                      </FaceButton>
                    )
                  }
                  sender={
                    (sender || plain) && (
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
                </MessageRun>
              </Fragment>
            );
          })}
        </div>
      ))}
      {writing.map((writer) => (
        <MessageRun
          key={writer.userId}
          direction="out"
          face={<Face name={writer.name} code={writer.code} photo={writer.photo} size="message" />}
          sender={<SenderLine name={writer.name} />}
        >
          <div className="typing" role="status" aria-label={`${writer.firstName} is typing`}>
            <i />
            <i />
            <i />
          </div>
        </MessageRun>
      ))}
    </ThreadFrame>
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
  return (
    <MessageMeta time={time && clock(message.at, timeZone)}>
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
    </MessageMeta>
  );
}
