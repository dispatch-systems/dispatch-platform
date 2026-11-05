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
import type { CommunicationsUpload, ComposeLimits } from '../../api/index.js';
import type { Attachments } from '../../api/client.js';
import { useFeedback } from '../../../../core/shell/frontend/runtime/feedback.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { messageOf } from '../../../../core/shell/frontend/lib/errors.js';
import { Shown } from '../display/display.js';
import { size } from '../shared/format.js';
import { randomId } from '../../../../core/shell/frontend/lib/random-id.js';
import type { Drafts } from './drafts.js';
import { EmojiPicker, finePointer } from './EmojiPicker.js';
import './composer.css';

// Writing a message, in a conversation or to a group: as much as one may carry, as the API says
// (`limits`): its text's characters, and its photos, how many, of which kinds and how large, which
// what refuses one says too. Photos upload as they're added, as the way it goes keeps them
// (`attachments`): by text, to the DSP's phones, which send them with it. Enter sends, Shift+Enter
// starts a new line; or, as the member may choose, Enter starts one and Ctrl or ⌘ with it sends.
// An emoji picked goes in where the writer was, or after the one picked before; with a mouse, the
// field takes focus back, where a phone's keyboard stays down.
// While someone writes in a conversation, their teammates see they are, said again every few
// seconds. A text says the number it goes to by its last digits, when the name doesn't. While what
// it goes to can't be reached, as a driver holding no work phone, what says so takes the box's
// place (`instead`), and what's written stays, with its photos, until it can be again. What's
// written is kept by the page (`drafts`), so it's there again as the box opens again; sent, what
// was written and photos added while it went stay. Above the box, who else is writing (`above`);
// and where a teammate replied since the member started writing (`replied`), sending asks there
// first, to send anyway or not.

/** How often writing is said again: the server counts it for six seconds. */
const TYPING_MS = 3000;

/** A photo added: shown at once, uploaded as it's added, and sent by what that answered. */
type Added = {
  key: string;
  file: File;
  shown: string;
  uploaded?: CommunicationsUpload;
  controller: AbortController;
};

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
  const { fail } = useFeedback();
  const text = useSyncExternalStore(
    useCallback((watch: () => void) => drafts.watch(draft, watch), [drafts, draft]),
    () => drafts.text(draft),
  );
  const { enter_sends: enterSends } = useContext(Shown);
  const [photos, setPhotos] = useState<Added[]>([]);
  // A teammate's reply, as it reads, asked about before sending.
  const [asking, setAsking] = useState<string>();
  // The photos of a send under way, which are no part of what's written meanwhile.
  const going = useRef<ReadonlySet<string>>(new Set());
  const picker = useRef<HTMLInputElement>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  const said = useRef(0);
  // Where the next emoji picked goes in, after the last, until the writer moves in the field.
  const caret = useRef<number>(undefined);
  const refocus = useRef(false);
  // Photos still uploading stop, and their previews go, as the composer does.
  const added = useRef(photos);
  useEffect(() => {
    added.current = photos;
  }, [photos]);
  useEffect(
    () => () => {
      for (const photo of added.current) {
        photo.controller.abort();
        URL.revokeObjectURL(photo.shown);
      }
    },
    [],
  );
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

  const addEmoji = (emoji: string) => {
    const element = field.current!;
    const start = caret.current ?? element.selectionStart;
    const end = caret.current ?? element.selectionEnd;
    const next = text.slice(0, start) + emoji + text.slice(end);
    if (limits && next.length > limits.text) return;
    caret.current = start + emoji.length;
    refocus.current = finePointer();
    writing(next);
  };
  // A draft starts as an empty box gains text or a photo, beside what's going: what a teammate's
  // reply counts from, and what was asked about before is no longer the question.
  const starts = () => {
    const empty = !text.trim() && photos.every((photo) => going.current.has(photo.key));
    if (!empty) return;
    replied?.seen();
    setAsking(undefined);
  };
  const remove = (key: string) => {
    const photo = added.current.find((each) => each.key === key);
    photo?.controller.abort();
    if (photo) URL.revokeObjectURL(photo.shown);
    setPhotos((now) => now.filter((each) => each.key !== key));
  };
  const add = (files: FileList | null) => {
    const chosen = [...(files ?? [])];
    if (!limits) return;
    if (chosen.some((file) => !limits.kinds.includes(file.type)))
      return fail(`Photos must be ${kindsText(limits.kinds)}.`);
    if (chosen.some((file) => file.size > limits.largest))
      return fail(`Photos must be ${bytesText(limits.largest)} or smaller.`);
    if (photos.length + chosen.length > limits.photos)
      return fail(`Add up to ${limits.photos} photos.`);
    if (chosen.length) starts();
    const adding = chosen.map((file) => ({
      key: randomId(),
      file,
      shown: URL.createObjectURL(file),
      controller: new AbortController(),
    }));
    setPhotos((now) => [...now, ...adding]);
    for (const photo of adding)
      attachments.add(photo.file, photo.controller.signal).then(
        (uploaded) =>
          setPhotos((now) =>
            now.map((each) => (each.key === photo.key ? { ...each, uploaded } : each)),
          ),
        (cause) => {
          if (photo.controller.signal.aborted) return;
          remove(photo.key);
          fail(messageOf(cause));
        },
      );
  };
  const send = useAction(async () => {
    const sent = new Set(photos.map((photo) => photo.key));
    going.current = sent;
    // What goes leaves the box at once; what's written and photos added while it goes stay.
    drafts.sending(draft, text);
    caret.current = undefined;
    try {
      await deliver(
        text.trim(),
        photos.map((photo) => photo.uploaded!.id),
      );
    } catch (cause) {
      drafts.failed(draft, text);
      throw cause;
    } finally {
      going.current = new Set();
    }
    for (const photo of photos) URL.revokeObjectURL(photo.shown);
    setPhotos((now) => now.filter((photo) => !sent.has(photo.key)));
    said.current = 0;
  });
  const uploading = photos.some((photo) => !photo.uploaded);
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
                add(event.currentTarget.files);
                event.currentTarget.value = '';
              }}
            />
          </>
        )}
        <EmojiPicker disabled={!open} pick={addEmoji} />
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

/** The kinds of photo a message carries, as their names read: "PNG, JPEG or GIF". */
function kindsText(kinds: readonly string[]) {
  const names = kinds.map((kind) => kind.replace(/^image\//, '').toUpperCase());
  return names.length > 1 ? `${names.slice(0, -1).join(', ')} or ${names.at(-1)}` : names.join('');
}
/** The most bytes a photo may be, as it reads: "20 MB", or with a fraction, "2.5 MB". */
function bytesText(bytes: number) {
  const megabytes = bytes / 1024 ** 2;
  return Number.isInteger(megabytes) ? `${megabytes} MB` : size(bytes);
}
