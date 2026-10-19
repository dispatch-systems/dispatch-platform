import { useRef, useState, type RefCallback } from 'react';
import type {
  SettingsField,
  SettingsSection,
  SettingsValue,
} from '../../../core/settings/api/index.js';
import { saveSettings } from '../../../core/settings/api/client.js';
import { ApiError } from '../../../core/shell/frontend/runtime/api.js';
import { useFeedback } from '../../../core/shell/frontend/runtime/feedback.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { useUnsavedEdits } from '../../../core/shell/frontend/runtime/useUnsavedEdits.js';
import { ConfirmDialog, ErrorBox } from '../../../core/shell/frontend/ui/index.js';
import { FieldRow, held, saved, valid } from './fields/index.js';

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/**
 * What marks a section's element, to find it by the section's id; given `reveal`, it is pointed
 * out (`data-found`), and `reveal` takes it once drawn.
 */
export const sectionElement = (id: string, reveal?: RefCallback<HTMLElement>) => ({
  id: `settings-section-${id}`,
  'data-settings-section': id,
  ...(reveal && { 'data-found': '', ref: reveal }),
});

/**
 * A declared section: its heading, a control for each of its fields and Save for those it holds,
 * then those that save themselves. Its edits are its own, so typing in one section redraws no
 * other, and the page moves on only once they are saved or discarded. A save that would lose
 * something asks first, in the server's words. Edits begin at the section as it was read when
 * they began: once another session has saved it since, a save of them is refused, though the
 * page has read it again meanwhile, and the section is read again.
 */
export function Section({
  section,
  refresh,
  reveal,
  named = false,
}: {
  section: SettingsSection;
  refresh: () => void;
  /** Points the section out, as a search result opening it does. */
  reveal?: RefCallback<HTMLElement>;
  /** Whether the page's heading already names it, as the only section of a page of its name. */
  named?: boolean;
}) {
  // Each field edited, by its id, and the revision of the section the edits began at.
  const [draft, setDraft] = useState<{ revision: number; values: Record<string, SettingsValue> }>();
  const [warning, setWarning] = useState<string>();
  const { notify } = useFeedback();
  const values = section.fields.filter(held);
  const before = (field: SettingsField) => section.values[field.id] ?? null;
  const edited = draft?.values ?? {};
  const holds = (field: SettingsField) => (field.id in edited ? edited[field.id]! : before(field));
  const changes = (edits: Record<string, SettingsValue>) =>
    values.filter(
      (field) => field.id in edits && !same(saved(field, edits[field.id]!), before(field)),
    );
  const changed = changes(edited);
  const ready =
    changed.length > 0 && values.every((field) => valid(field, holds(field), before(field)));
  // Edits begin at the section as read now, unless some made before are still unsaved.
  const edit = (field: SettingsField, value: SettingsValue) =>
    setDraft((old) => ({
      revision: old && changes(old.values).length > 0 ? old.revision : section.revision,
      values: { ...old?.values, [field.id]: value },
    }));
  // Whether the latest save stored the edits, rather than failing or asking first.
  const stored = useRef(false);
  const save = useAction(
    async (confirmed: boolean) => {
      stored.current = false;
      try {
        const answer = await saveSettings(section.id, {
          values: Object.fromEntries(
            changed.map((field) => [field.id, saved(field, edited[field.id]!)]),
          ),
          revision: draft!.revision,
          ...(confirmed ? { confirmed } : {}),
        });
        if ('warning' in answer) return setWarning(answer.warning);
        setWarning(undefined);
        setDraft(undefined);
        stored.current = true;
        notify(`${section.label} saved`);
      } catch (cause) {
        setWarning(undefined);
        // Another session saved it first: what it holds now replaces the edits.
        if (cause instanceof ApiError && cause.code === 'settings_changed_reload_before_saving') {
          setDraft(undefined);
          refresh();
        }
        throw cause;
      }
    },
    { inline: true },
  );
  const unsaved = useUnsavedEdits({
    what: section.label,
    dirty: changed.length > 0,
    saveable: ready && !save.busy,
    save: async () => (await save.run(false)) && stored.current,
  });
  const element = sectionElement(section.id, reveal);
  const heading = `${element.id}-heading`;
  const row = (field: SettingsField) => (
    <FieldRow
      key={field.id}
      field={field}
      id={`${element.id}-${field.id}`}
      value={holds(field)}
      before={before(field)}
      change={(value) => edit(field, value)}
      alone={section.fields.length === 1}
    />
  );
  return (
    <section className="settings-section" aria-labelledby={heading} {...element}>
      <h2 id={heading} className={named ? 'sr-only' : undefined}>
        {section.label}
      </h2>
      {values.length > 0 && (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (ready && !save.busy) void save.run(false);
          }}
        >
          <fieldset className="settings-fields" disabled={save.busy}>
            {values.map(row)}
          </fieldset>
          <div className="settings-section-actions">
            <ErrorBox message={save.error} />
            <button type="submit" className="primary" disabled={!ready || save.busy}>
              {save.busy ? 'Saving…' : 'Save'}
            </button>
          </div>
        </form>
      )}
      {/* Each in a form of its own, outside the section's. */}
      {section.fields.filter((field) => !held(field)).map(row)}
      {warning && (
        <ConfirmDialog
          title={section.label}
          confirm="Save"
          tone="danger"
          busy={save.busy}
          onCancel={() => setWarning(undefined)}
          onConfirm={() => void save.run(true)}
        >
          {warning}
        </ConfirmDialog>
      )}
      {unsaved.asking}
    </section>
  );
}
