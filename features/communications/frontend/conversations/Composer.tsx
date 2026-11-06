import {
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import { ArrowUp, LoaderCircle, Paperclip, Phone, X } from 'lucide-react';
import type { ComposeLimits } from '../../api/index.js';
import type { Attachments } from '../../api/client.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { Shown } from '../display/display.js';
import type { Drafts } from './drafts.js';
import { useComposerPhotos } from './useComposerPhotos.js';
import { EmojiPicker, finePointer } from './EmojiPicker.js';
import { QuickReplies } from '../replies/QuickReplies.js';
import './composer.css';

// Text editing, typing and reply acknowledgement, with composer-local photo ownership.

/** How often writing is said again: the server counts it for six seconds. */
const TYPING_MS = 3000;

export function Composer({
  drafts,
  draft,
  placeholder,
  writable,
  ready: whom = true,
  limits,
  attachments,
  to,
  send: deliver,
  typing,
  label,
  instead,
  above,
  replied,
}: {
  /** What's written in each box, kept by the page. */
  drafts: Drafts;
  /** Which of them this box's is. */
  draft: string;
  placeholder: string;
  /** Whether it can be written in: Communications works where it goes now. */
  writable: boolean;
  /** What a message may carry; none while that isn't known, when nothing can be written. */
  limits: ComposeLimits | undefined;
  /** How the photos added are kept, by the way it goes: in Dispatch, or by the DSP's phones. */
  attachments: Attachments;
  /** The last digits of the number a text goes to, when the name doesn't say it. */
  to?: string;
  /** Whether what's written can go, as once there's someone to send it to. */
  ready?: boolean;
  /** Sends what's written with the photos added for it, by their IDs. */
  send: (text: string, photos: string[]) => Promise<void>;
  /** Says the member is writing, for the next few seconds. */
  typing?: () => void;
  /** What sending says beside its arrow, as "Send to 8"; else "Send". */
  label?: string;
  /** What shows in the box's place while what it goes to can't be reached. */
  instead?: ReactNode;
  /** What shows right above the box, as who else is writing. */
  above?: ReactNode;
  /** Before sending, what a teammate's reply since the member started writing says, if there's
   * one (`check`); told as the member saw what's there now (`seen`), and to show the reply when
   * they'd rather not send (`show`). */
  replied?: { check: () => string | undefined; seen: () => void; show: () => void };
}) {
  const text = useSyncExternalStore(
    useCallback((watch: () => void) => drafts.watch(draft, watch), [drafts, draft]),
    () => drafts.text(draft),
  );
  const { enter_sends: enterSends } = useContext(Shown);
  const {
    photos,
    add,
    remove,
    uploading,
    empty: photosEmpty,
    capture,
    finish,
    settle,
  } = useComposerPhotos(limits, attachments);
  // A teammate's reply, as it reads, asked about before sending.
  const [asking, setAsking] = useState<string>();
  const picker = useRef<HTMLInputElement>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  const said = useRef(0);
  // Where the next emoji or quick reply picked goes in, after the last, until the writer moves in
  // the field.
  const caret = useRef<number>(undefined);
  const refocus = useRef(false);
  // The field grows with what's written, up to a few lines, and as it's drawn again with what was
  // kept while something else took its place.
  const away = Boolean(instead);
  useEffect(() => {
    const element = field.current;
    if (!element) return;
    element.style.height = 'auto';
    element.style.height = `${element.scrollHeight}px`;
  }, [text, away]);

  useLayoutEffect(() => {
    if (!refocus.current || caret.current === undefined) return;
    refocus.current = false;
    field.current!.focus();
    field.current!.setSelectionRange(caret.current, caret.current);
  }, [text]);

  const put = (picked: string) => {
    const element = field.current!;
    const start = caret.current ?? element.selectionStart;
    const end = caret.current ?? element.selectionEnd;
    const next = text.slice(0, start) + picked + text.slice(end);
    if (limits && next.length > limits.text) return;
    caret.current = start + picked.length;
    refocus.current = finePointer();
    writing(next);
  };
  // A draft starts as an empty box gains text or a photo, beside what's going: what a teammate's
  // reply counts from, and what was asked about before is no longer the question.
  const starts = () => {
    const empty = !text.trim() && photosEmpty();
    if (!empty) return;
    replied?.seen();
    setAsking(undefined);
  };
  const send = useAction(async () => {
    const sent = capture();
    // What goes leaves the box at once; what's written and photos added while it goes stay.
    drafts.sending(draft, text);
    caret.current = undefined;
    try {
      await deliver(
        text.trim(),
        sent.photos.map((photo) => photo.uploaded!.id),
      );
    } catch (cause) {
      drafts.failed(draft, text);
      throw cause;
    } finally {
      finish();
    }
    settle(sent);
    said.current = 0;
  });
  const open = writable && limits !== undefined;
  const ready =
    open && whom && !send.busy && !uploading && (text.trim().length > 0 || photos.length > 0);
  // Sends, unless a teammate replied since the member started writing: that's asked first.
  const go = () => {
    const reply = replied?.check();
    setAsking(reply);
    if (!reply) void send.run();
  };
  // Answered: sent anyway, only once it can go as Send would, or not.
  const answer = (sending: boolean) => {
    if (sending && !ready) return;
    setAsking(undefined);
    replied?.seen();
    if (sending) void send.run();
    else replied?.show();
  };
  const writing = (value: string) => {
    if (value.trim()) starts();
    drafts.write(draft, value);
    if (typing && value.trim() && Date.now() - said.current > TYPING_MS) {
      said.current = Date.now();
      typing();
    }
  };
  const sending = send.busy ? <LoaderCircle className="spin" size={18} /> : <ArrowUp size={18} />;
  if (instead) return <div className="composer">{instead}</div>;
  return (
    <form
      className="composer"
      onSubmit={(event) => {
        event.preventDefault();
        if (ready) go();
      }}
    >
      {above}
      {asking && (
        <div className="replied" role="alert">
          <strong>{asking}</strong>
          <button type="button" className="primary" disabled={!ready} onClick={() => answer(true)}>
            Send anyway
          </button>
          <button type="button" onClick={() => answer(false)}>
            Cancel
          </button>
        </div>
      )}
      {photos.length > 0 && (
        <ul className="adding">
          {photos.map((photo) => (
            <li key={photo.key}>
              <img src={photo.shown} alt={photo.file.name} />
              {!photo.uploaded && <LoaderCircle className="spin" size={18} aria-hidden="true" />}
              <button
                type="button"
                aria-label={`Remove ${photo.file.name}`}
                onClick={() => remove(photo.key)}
              >
                <X size={12} />
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className={label && text ? 'box writing' : 'box'}>
        {limits && limits.photos > 0 && (
          <>
            <button
              type="button"
              className="round"
              aria-label="Add photo"
              title="Add photo"
              disabled={!open || photos.length >= limits.photos}
              onClick={() => picker.current?.click()}
            >
              <Paperclip size={18} />
            </button>
            <input
              ref={picker}
              type="file"
              accept={limits.kinds.join(',')}
              multiple
              hidden
              onChange={(event) => {
                add(event.currentTarget.files, starts);
                event.currentTarget.value = '';
              }}
            />
          </>
        )}
        <EmojiPicker disabled={!open} pick={put} />
        <QuickReplies disabled={!open} pick={put} />
        <textarea
          ref={field}
          rows={1}
          aria-label="Message"
          placeholder={placeholder}
          maxLength={limits?.text}
          value={text}
          disabled={!open}
          onChange={(event) => writing(event.target.value)}
          onSelect={() => {
            caret.current = undefined;
          }}
          onKeyDown={(event) => {
            if (event.key !== 'Enter' || event.shiftKey || event.nativeEvent.isComposing) return;
            if (!enterSends && !event.ctrlKey && !event.metaKey) return;
            event.preventDefault();
            if (ready) go();
          }}
        />
        {to && (
          <span className="texting-to">
            <Phone size={12} aria-hidden="true" />
            {to}
          </span>
        )}
        <button type="submit" className="send-to" disabled={!ready}>
          {label ?? 'Send'}
          {sending}
        </button>
      </div>
    </form>
  );
}
