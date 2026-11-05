import type { Writing } from '../../api/index.js';
import { typingNote } from '../shared/format.js';
import './typing-line.css';

/** Who else at the DSP is writing in the conversation, "Keisha is typing…", right above its box
 * however its messages are scrolled; said to a screen reader as it changes. */
export function TypingLine({ writing }: { writing: readonly Writing[] }) {
  return (
    <p className="typing-line" role="status">
      {writing.length > 0 && typingNote(writing)}
    </p>
  );
}
