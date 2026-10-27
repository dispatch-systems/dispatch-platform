import { useContext, useEffect, useRef, useState } from 'react';
import { ArrowUp, LoaderCircle, Paperclip, Phone, X } from 'lucide-react';
import type { CommunicationsUpload, ComposeLimits } from '../../api/index.js';
import type { Attachments } from '../../api/client.js';
import { useFeedback } from '../../../../core/shell/frontend/runtime/feedback.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { messageOf } from '../../../../core/shell/frontend/lib/errors.js';
import { Shown } from '../display/display.js';
import { size } from '../shared/format.js';
import { randomId } from '../../../../core/shell/frontend/lib/random-id.js';
import './composer.css';

// Writing a message, in a conversation or to a group: as much as one may carry, as the API says
// (`limits`): its text's characters, and its photos, how many, of which kinds and how large, which
// what refuses one says too. Photos upload as they're added, as the way it goes keeps them
// (`attachments`): by text, to the DSP's phones, which send them with it. Enter sends, Shift+Enter
// starts a new line; or, as the member may choose, Enter starts one and Ctrl or ⌘ with it sends.
// While someone writes in a conversation, their teammates see they are, said again every few
// seconds. A text says the number it goes to by its last digits, when the name doesn't.

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
  placeholder,
  writable,
  ready: whom = true,
  limits,
  attachments,
  to,
  send: deliver,
  typing,
  label,
}: {
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
  /** What sending says beside its arrow, as "Send to 8"; else the arrow alone. */
  label?: string;
}) {
  const { fail } = useFeedback();
  const [text, setText] = useState('');
  const { enter_sends: enterSends } = useContext(Shown);
  const [photos, setPhotos] = useState<Added[]>([]);
  const picker = useRef<HTMLInputElement>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  const said = useRef(0);
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
  // The field grows with what's written, up to a few lines.
  useEffect(() => {
    const element = field.current;
    if (!element) return;
    element.style.height = 'auto';
    element.style.height = `${element.scrollHeight}px`;
  }, [text]);

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
    await deliver(
      text.trim(),
      photos.map((photo) => photo.uploaded!.id),
    );
    for (const photo of photos) URL.revokeObjectURL(photo.shown);
    setPhotos([]);
    setText('');
    said.current = 0;
  });
  const uploading = photos.some((photo) => !photo.uploaded);
  const open = writable && limits !== undefined;
  const ready =
    open && whom && !send.busy && !uploading && (text.trim().length > 0 || photos.length > 0);
  const writing = (value: string) => {
    setText(value);
    if (typing && value.trim() && Date.now() - said.current > TYPING_MS) {
      said.current = Date.now();
      typing();
    }
  };
  const sending = send.busy ? <LoaderCircle className="spin" size={18} /> : <ArrowUp size={18} />;
  return (
    <form
      className="composer"
      onSubmit={(event) => {
        event.preventDefault();
        if (ready) void send.run();
      }}
    >
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
              aria-label="Attach photos"
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
        <textarea
          ref={field}
          rows={1}
          aria-label="Message"
          placeholder={placeholder}
          maxLength={limits?.text}
          value={text}
          disabled={!open}
          onChange={(event) => writing(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== 'Enter' || event.shiftKey || event.nativeEvent.isComposing) return;
            if (!enterSends && !event.ctrlKey && !event.metaKey) return;
            event.preventDefault();
            if (ready) void send.run();
          }}
        />
        {to && (
          <span className="texting-to">
            <Phone size={12} aria-hidden="true" />
            {to}
          </span>
        )}
        {label ? (
          <button type="submit" className="send-to" disabled={!ready}>
            {label}
            {sending}
          </button>
        ) : (
          <button type="submit" className="round send" aria-label="Send" disabled={!ready}>
            {sending}
          </button>
        )}
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
