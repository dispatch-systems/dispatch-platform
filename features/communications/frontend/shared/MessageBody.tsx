import { useEffect, useRef } from 'react';
import { FileText, LoaderCircle, Play } from 'lucide-react';
import type { Message, MessageFile } from '../../api/index.js';
import type { Attachments } from '../../api/client.js';
import { download, picture } from '../../../../core/shell/frontend/runtime/transfer.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { usePicture } from '../../../../core/shell/frontend/runtime/usePicture.js';
import { fileStatus, isPhoto, isVideo, size } from './format.js';
import { useViewPhoto, useThreadActivity } from './PhotoViewer.js';
import { shownAt } from '../gifs/klipy.js';
import './message-body.css';

/** How wide a GIF shows, in pixels, at most. */
const GIF_WIDTH = 240;
/** The most bytes of a video the page plays; a larger one is downloaded. */
const PLAYS_MOST = 25 * 1024 ** 2;

/** One message: its GIF, shown from KLIPY, its text, marked if edited in Slack, then its photos,
 * videos and files, read from where the way it went keeps them (`attachments`). */
export function MessageBody({
  message,
  attachments,
  identity,
}: {
  identity?: string;
  message: Pick<Message, 'text' | 'editedAt' | 'files'> & Partial<Pick<Message, 'status' | 'gif'>>;
  /** Where a photo, video or file of it is read from, by the way it went. */
  attachments: Pick<Attachments, 'address' | 'preview' | 'fetch'>;
}) {
  const photos = message.files.filter((file) => file.status === 'kept' && isPhoto(file.mime));
  const videos = message.files.filter(
    (file) =>
      isVideo(file.mime) &&
      file.size <= PLAYS_MOST &&
      (file.status === 'kept' || (file.status === 'on_phone' && attachments.fetch)),
  );
  const files = message.files.filter((file) => !photos.includes(file) && !videos.includes(file));
  const gif = message.gif && shownAt(message.gif, GIF_WIDTH);
  return (
    <>
      {message.gif && gif && (
        <img
          data-message-id={identity}
          data-message-part="gif"
          className="gif"
          src={gif.url}
          alt={message.gif.title || 'GIF'}
          style={{ aspectRatio: `${gif.width} / ${gif.height}` }}
        />
      )}
      {message.text && (
        <div
          data-message-id={identity}
          data-message-part="text"
          className={message.status === 'failed' ? 'bubble failed' : 'bubble'}
        >
          {message.text}
          {message.editedAt !== null && <span className="edited">Edited</span>}
        </div>
      )}
      {photos.map((photo) => (
        <Photo identity={identity} key={photo.id} photo={photo} address={attachments.address} />
      ))}
      {videos.map((video) => (
        <Video identity={identity} key={video.id} video={video} attachments={attachments} />
      ))}
      {files.map((file) => (
        <Attached identity={identity} key={file.id} file={file} address={attachments.address} />
      ))}
    </>
  );
}

/**
 * A photo, its preview fetched as it nears view, as the page's own calls are, with the DSP's
 * view; pressed, it opens over the page at full size.
 */
function Photo({
  identity,
  photo,
  address,
}: {
  identity?: string;
  photo: MessageFile;
  address: (id: string) => string;
}) {
  const url = versioned(photo, address);
  const [box, src, lost] = usePicture<HTMLButtonElement>(
    versioned(photo, (id) => `${address(id)}/preview`),
  );
  const view = useViewPhoto();
  return (
    <button
      ref={box}
      data-message-id={identity}
      data-message-part={`file:${photo.id}`}
      className="photo"
      disabled={!src}
      onClick={() => src && view({ name: photo.name, src, url })}
    >
      {src && <img src={src} alt={photo.name} onError={lost} />}
    </button>
  );
}

/** Where `file` is read from, at the version of its bytes, so the browser keeps it. */
const versioned = (file: MessageFile, address: (id: string) => string) =>
  file.version ? `${address(file.id)}?v=${file.version}` : address(file.id);

/**
 * A video: the picture of its first frame, where its phone sent one, under a play button.
 * Pressed, it's asked for, from its phone where it keeps it, as the server says it stands now
 * rather than as the message last said; then fetched whole, and played over the page, or
 * downloaded where the browser can't play it. Unmounted as it's fetched, it's left.
 */
function Video({
  video,
  attachments,
  identity,
}: {
  identity?: string;
  video: MessageFile;
  attachments: Pick<Attachments, 'address' | 'preview' | 'fetch'>;
}) {
  const view = useViewPhoto();
  const url = attachments.address(video.id);
  const [box, still, lost] = usePicture<HTMLButtonElement>(
    video.preview ? attachments.preview?.(video.id) : undefined,
  );
  const active = useThreadActivity();
  const playing = useRef<AbortController>(undefined);
  useEffect(() => () => playing.current?.abort(), []);
  const play = useAction(async () => {
    const release = active();
    const stop = new AbortController();
    playing.current = stop;
    try {
      const asked = attachments.fetch && (await attachments.fetch(video.id, stop.signal));
      if (asked && !asked.kept) throw new Error("The phone didn't send the video. Try again.");
      // One the browser surely can't play; one it may not, the viewer saves if it can't.
      if (document.createElement('video').canPlayType(video.mime) === '')
        return await download(url);
      const bytes = await picture(url, stop.signal);
      if (!stop.signal.aborted)
        view({ name: video.name, src: URL.createObjectURL(bytes), video: true });
    } catch (error) {
      if (!stop.signal.aborted) throw error;
    } finally {
      release();
    }
  });
  return (
    <button
      ref={box}
      data-message-id={identity}
      data-message-part={`file:${video.id}`}
      className="photo video"
      aria-label={`Play ${video.name}`}
      aria-busy={play.busy}
      disabled={play.busy}
      onClick={() => void play.run()}
    >
      {still && <img src={still} alt="" onError={lost} />}
      <span className="play">
        {play.busy ? (
          <LoaderCircle className="spin" size={22} aria-hidden="true" />
        ) : (
          <Play size={22} aria-hidden="true" />
        )}
      </span>
    </button>
  );
}

/** A file that isn't a photo, or one Dispatch doesn't keep: its name, and its size or why. */
function Attached({
  identity,
  file,
  address,
}: {
  identity?: string;
  file: MessageFile;
  address: (id: string) => string;
}) {
  const active = useThreadActivity();
  const save = useAction(async () => {
    const release = active();
    try {
      await download(versioned(file, address));
    } finally {
      release();
    }
  });
  const about = (
    <>
      <FileText size={18} aria-hidden="true" />
      <span className="file-name">{file.name}</span>
      <small>{file.status === 'kept' ? size(file.size) : fileStatus[file.status]}</small>
    </>
  );
  return file.status === 'kept' ? (
    <button
      data-message-id={identity}
      data-message-part={`file:${file.id}`}
      className="file"
      disabled={save.busy}
      onClick={() => void save.run()}
    >
      {about}
    </button>
  ) : (
    <span data-message-id={identity} data-message-part={`file:${file.id}`} className="file unkept">
      {about}
    </span>
  );
}
