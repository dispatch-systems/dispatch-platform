import type { FileStatus, Message, TextStatus } from '../api/index.js';
import { dateFormatter } from '../../../core/shell/frontend/lib/date-format.js';
import { localDate, shiftDate } from '../../../core/shell/frontend/lib/day.js';
import { phoneText } from '../../../core/shell/frontend/lib/format.js';

// How the page words times, waits and files, in the DSP's timezone so everyone reads one clock.

/** "7:31 AM". */
export const clock = (at: number, timeZone: string) =>
  dateFormatter('en-US', { hour: 'numeric', minute: '2-digit', timeZone }).format(at);

/** How far back `at` is from today, in whole days, and the day it falls on. */
function daysBack(at: number, timeZone: string, now: number) {
  const day = localDate(timeZone, new Date(at));
  const today = localDate(timeZone, new Date(now));
  let back = 0;
  while (back < 7 && shiftDate(today, -back) !== day) back++;
  return { back, sameYear: day.slice(0, 4) === today.slice(0, 4) };
}
/** "Oct 15", with its year outside this one. */
const date = (at: number, timeZone: string, sameYear: boolean) =>
  dateFormatter('en-US', {
    month: 'short',
    day: 'numeric',
    ...(sameYear ? {} : { year: 'numeric' }),
    timeZone,
  }).format(at);

/** When a conversation's latest message was written, as its row shows it. */
export function listTime(at: number, timeZone: string, now = Date.now()) {
  const { back, sameYear } = daysBack(at, timeZone, now);
  if (back === 0) return clock(at, timeZone);
  if (back === 1) return 'Yesterday';
  if (back < 7) return dateFormatter('en-US', { weekday: 'short', timeZone }).format(at);
  return date(at, timeZone, sameYear);
}
/** The day messages were written on, above them in a conversation. */
export function dayLabel(at: number, timeZone: string, now = Date.now()) {
  const { back, sameYear } = daysBack(at, timeZone, now);
  if (back === 0) return 'Today';
  if (back === 1) return 'Yesterday';
  if (back < 7) return dateFormatter('en-US', { weekday: 'long', timeZone }).format(at);
  return date(at, timeZone, sameYear);
}
/** The day `at` falls on, to tell messages of one day from the next. */
export const dayOf = (at: number, timeZone: string) => localDate(timeZone, new Date(at));

/** How long someone has waited on an answer, and how late that is: amber from 15 minutes, red from 30. */
export function waited(since: number, now = Date.now()) {
  const minutes = Math.max(0, Math.floor((now - since) / 60_000));
  const label =
    minutes < 60
      ? `${minutes}m`
      : minutes < 24 * 60
        ? `${Math.floor(minutes / 60)}h`
        : `${Math.floor(minutes / (24 * 60))}d`;
  return { label, tone: minutes >= 30 ? 'late' : minutes >= 15 ? 'slow' : '' };
}

/** A file that isn't kept in Dispatch, by why. */
export const fileStatus: Record<Exclude<FileStatus, 'kept'>, string> = {
  waiting: 'Downloading',
  too_large: 'Too large',
  unavailable: 'Hosted elsewhere',
  failed: "Couldn't download",
};
/** How a text Dispatch sent went, as it reads. */
export const textStatus: Record<TextStatus, string> = {
  waiting: 'Sending',
  sent: 'Sent',
  delivered: 'Delivered',
  failed: 'Not sent',
};
/** A number as it reads beside a name, while the name doesn't say it already: none when it does. */
export function numberBeside(name: string, number: string) {
  const shown = phoneText(number);
  return name === shown ? '' : shown;
}
/** "2.4 MB". */
export function size(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
}
/** Whether a file is a photo the page shows. */
export const isPhoto = (mime: string) => /^image\/(png|jpeg|gif|webp)$/.test(mime);

/** A message as a row of the list shows it: its text, else what it holds. */
export function preview(message: Pick<Message, 'text' | 'files'>) {
  if (message.text.trim()) return message.text;
  const [first] = message.files;
  if (!first) return '';
  return message.files.every((file) => isPhoto(file.mime))
    ? message.files.length > 1
      ? `${message.files.length} photos`
      : 'Photo'
    : first.name;
}

/** The identity tone someone's initials are drawn in. */
export const tone = (name: string) =>
  ['violet', 'sage', 'slate', 'steel'][
    [...name].reduce((hash, ch) => (hash * 31 + ch.codePointAt(0)!) >>> 0, 0) % 4
  ]!;

/** The letter a name is listed under, A to Z: its first, without its accent; "#" for none. */
export function letter(name: string) {
  const first = [...name.trim().normalize('NFD')][0]?.toLocaleUpperCase('en-US') ?? '';
  return /\p{L}/u.test(first) ? first : '#';
}
