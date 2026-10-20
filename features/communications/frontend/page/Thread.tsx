import { Fragment, useLayoutEffect, useRef } from 'react';
import { FileText } from 'lucide-react';
import type { Conversation, Message, MessageFile, Writing } from '../../api/index.js';
import { fileUrl } from '../../api/client.js';
import { clock, dayLabel, dayOf, fileStatus, isPhoto, size } from '../format.js';
import { download } from '../../../../core/shell/frontend/runtime/transfer.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { usePicture } from '../../../../core/shell/frontend/runtime/usePicture.js';
import { Face } from './Face.js';
import { More } from './More.js';

// A conversation's messages, in the order written, under the day each was written: the
// member's on the left beside their photo, the DSP's on the right under the name and photo of
// whoever sent them; and below, those at the DSP writing in it now.

/** Messages one after another from the same writer, within a few minutes, shown together. */
type Run = { key: string; writer: string; messages: Message[] };
const TOGETHER_MS = 10 * 60_000;

/** The messages under each day they were written, in runs by writer. */
function runs(messages: readonly Message[], timeZone: string) {
  const days: { day: string; at: number; runs: Run[] }[] = [];
  for (const message of messages) {
    const day = dayOf(message.at, timeZone);
    if (days.at(-1)?.day !== day) days.push({ day, at: message.at, runs: [] });
    const current = days.at(-1)!;
    const writer = message.direction === 'in' ? 'in' : `out:${message.sender?.userId ?? ''}`;
    const run = current.runs.at(-1);
    if (run?.writer === writer && message.at - run.messages.at(-1)!.at < TOGETHER_MS)
      run.messages.push(message);
    else current.runs.push({ key: message.id, writer, messages: [message] });
  }
  return days;
}

export function Thread({
  conversation,
  messages,
  more,
  readOlder,
  writing,
  timeZone,
  now,
}: {
  conversation: Conversation;
  messages: readonly Message[];
  more: boolean;
  readOlder: () => void;
  writing: readonly Writing[];
  timeZone: string;
  now: number;
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
  return (
    <div className="thread" ref={scroller} onScroll={scrolled}>
      {more && <More read={readOlder} />}
      <div className="thread-messages">
        {runs(messages, timeZone).map(({ day, at, runs }) => (
          <Fragment key={day}>
            <div className="day">{dayLabel(at, timeZone, now)}</div>
            {runs.map(({ key, messages: written }) => {
              const first = written[0]!;
              const sender = first.sender;
              return (
                <div key={key} className={`msg-group ${first.direction}`}>
                  {sender ? (
                    <Face name={sender.name} code={sender.code} photo={sender.photo} size={30} />
                  ) : (
                    <Face {...person} size={30} />
                  )}
                  <div className="stack">
                    {sender && <div className="sender">{sender.name}</div>}
                    {written.map((message) => (
                      <Written key={message.id} message={message} />
                    ))}
                    <div className="meta">{clock(written.at(-1)!.at, timeZone)}</div>
                  </div>
                </div>
              );
            })}
          </Fragment>
        ))}
        {writing.map((writer) => (
          <div key={writer.userId} className="msg-group out">
            <Face name={writer.name} code={writer.code} photo={writer.photo} size={30} />
            <div className="stack">
              <div className="sender">{writer.name}</div>
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

/** One message: its text, marked if edited in Slack, then its photos and files. */
function Written({ message }: { message: Message }) {
  const photos = message.files.filter((file) => file.status === 'kept' && isPhoto(file.mime));
  const files = message.files.filter((file) => !photos.includes(file));
  return (
    <>
      {message.text && (
        <div className="bubble">
          {message.text}
          {message.editedAt !== null && <span className="edited">Edited</span>}
        </div>
      )}
      {photos.map((photo) => (
        <Photo key={photo.id} photo={photo} />
      ))}
      {files.map((file) => (
        <Attached key={file.id} file={file} />
      ))}
    </>
  );
}

/**
 * A photo, fetched as it nears view, as the page's own calls are, with the DSP's view; it opens
 * at full size in a tab of its own.
 */
function Photo({ photo }: { photo: MessageFile }) {
  const [box, src, lost] = usePicture<HTMLAnchorElement>(fileUrl(photo.id));
  return (
    <a ref={box} className="photo" href={src} target="_blank" rel="noreferrer">
      {src && <img src={src} alt={photo.name} onError={lost} />}
    </a>
  );
}

/** A file that isn't a photo, or one Dispatch doesn't keep: its name, and its size or why. */
function Attached({ file }: { file: MessageFile }) {
  const save = useAction(() => download(fileUrl(file.id)));
  const about = (
    <>
      <FileText size={18} aria-hidden="true" />
      <span className="file-name">{file.name}</span>
      <small>{file.status === 'kept' ? size(file.size) : fileStatus[file.status]}</small>
    </>
  );
  return file.status === 'kept' ? (
    <button className="file" disabled={save.busy} onClick={() => void save.run()}>
      {about}
    </button>
  ) : (
    <span className="file unkept">{about}</span>
  );
}
