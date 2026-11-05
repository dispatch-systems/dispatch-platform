import { useState } from 'react';
import { Plus, X } from 'lucide-react';
import { saveQuickReplies, useQuickReplies } from '../../api/client.js';
import { ApiError } from '../../../../core/shell/frontend/runtime/api.js';
import { useFeedback } from '../../../../core/shell/frontend/runtime/feedback.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { useUnsavedEdits } from '../../../../core/shell/frontend/runtime/useUnsavedEdits.js';
import { randomId } from '../../../../core/shell/frontend/lib/random-id.js';
import { DataState, ErrorBox } from '../../../../core/shell/frontend/ui/index.js';
import './quick-replies-settings.css';

// Settings' Quick replies, on Communications' own page: the DSP's short answers, edited by anyone
// who uses Communications, each in a box of its own, added and removed, then saved together, the
// empty ones left out. Edits begin at the list as it was read: once another session has saved it
// since, a save of them is refused and the list is read again.

/** The most a DSP keeps, and the most characters each holds, as the server allows. */
export const MOST_REPLIES = 30;
export const LONGEST_REPLY = 500;

/** Each reply as edited, by a key of its own while it's edited, and the revision of the list the
 * edits began at. */
type Edits = { replies: Reply[]; revision: number };
type Reply = { key: string; text: string };

export function QuickRepliesSettings() {
  const read = useQuickReplies();
  const saved = read.data;
  const { notify } = useFeedback();
  const [edits, setEdits] = useState<Edits>();
  const replies =
    edits?.replies ?? saved?.replies.map((text, n) => ({ key: `saved-${n}`, text })) ?? [];
  const kept = replies.map((reply) => reply.text.trim()).filter(Boolean);
  const dirty = Boolean(edits) && JSON.stringify(kept) !== JSON.stringify(saved?.replies ?? []);
  const edit = (next: Reply[]) =>
    setEdits((old) => ({ replies: next, revision: old?.revision ?? saved?.revision ?? 0 }));
  const save = useAction(
    async () => {
      try {
        await saveQuickReplies(kept, edits!.revision);
        setEdits(undefined);
        notify('Quick replies saved');
      } catch (cause) {
        // Another session saved first: what it holds now replaces the edits.
        if (cause instanceof ApiError && cause.code === 'settings_changed_reload_before_saving') {
          setEdits(undefined);
          read.refresh();
        }
        throw cause;
      }
    },
    { inline: true },
  );
  const unsaved = useUnsavedEdits({
    what: 'Quick replies',
    dirty,
    saveable: !save.busy,
    save: () => save.run(),
  });
  return (
    <section className="settings-section quick-replies-settings" aria-labelledby="quick-replies">
      <h2 id="quick-replies">Quick replies</h2>
      <DataState data={saved} error={read.error} retry={read.refresh}>
        {() => (
          <form
            onSubmit={(event) => {
              event.preventDefault();
              if (dirty && !save.busy) void save.run();
            }}
          >
            <fieldset className="settings-fields" disabled={save.busy}>
              {replies.length > 0 && (
                <ol>
                  {replies.map((reply, n) => (
                    <li key={reply.key}>
                      <textarea
                        rows={2}
                        aria-label={`Quick reply ${n + 1}`}
                        maxLength={LONGEST_REPLY}
                        value={reply.text}
                        onChange={(event) =>
                          edit(
                            replies.map((each) =>
                              each.key === reply.key ? { ...each, text: event.target.value } : each,
                            ),
                          )
                        }
                      />
                      <button
                        type="button"
                        aria-label={`Remove quick reply ${n + 1}`}
                        onClick={() => edit(replies.filter((each) => each.key !== reply.key))}
                      >
                        <X size={15} />
                        <span>Remove</span>
                      </button>
                    </li>
                  ))}
                </ol>
              )}
            </fieldset>
            <div className="settings-section-actions">
              <ErrorBox message={save.error} />
              <button
                type="button"
                disabled={save.busy || replies.length >= MOST_REPLIES}
                onClick={() => edit([...replies, { key: randomId(), text: '' }])}
              >
                <Plus size={15} />
                <span>Add reply</span>
              </button>
              <button type="submit" className="primary" disabled={!dirty || save.busy}>
                {save.busy ? 'Saving…' : 'Save'}
              </button>
            </div>
          </form>
        )}
      </DataState>
      {unsaved.asking}
    </section>
  );
}
