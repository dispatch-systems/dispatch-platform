import { useState, type RefCallback } from 'react';
import type {
  SettingsField,
  SettingsSection,
  SettingsValue,
} from '../../../core/settings/api/index.js';
import { saveSettings } from '../../../core/settings/api/client.js';
import { ApiError } from '../../../core/shell/frontend/runtime/api.js';
import { useFeedback } from '../../../core/shell/frontend/runtime/feedback.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
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
 * other. A save that would lose something asks first, in the server's words; one read before
 * another session saved is refused, and the section is read again.
 */
export function Section({
  section,
  refresh,
  reveal,
}: {
  section: SettingsSection;
  refresh: () => void;
  /** Points the section out, as a search result opening it does. */
  reveal?: RefCallback<HTMLElement>;
}) {
  const [draft, setDraft] = useState<Record<string, SettingsValue>>({});
  const [warning, setWarning] = useState<string>();
  const { notify } = useFeedback();
  const values = section.fields.filter(held);
  const before = (field: SettingsField) => section.values[field.id] ?? null;
  const holds = (field: SettingsField) => (field.id in draft ? draft[field.id]! : before(field));
  const changed = values.filter(
    (field) => field.id in draft && !same(saved(field, draft[field.id]!), before(field)),
  );
  const ready =
    changed.length > 0 && values.every((field) => valid(field, holds(field), before(field)));
  const save = useAction(
    async (confirmed: boolean) => {
      try {
        const answer = await saveSettings(section.id, {
          values: Object.fromEntries(
            changed.map((field) => [field.id, saved(field, draft[field.id]!)]),
          ),
          revision: section.revision,
          ...(confirmed ? { confirmed } : {}),
        });
        if ('warning' in answer) return setWarning(answer.warning);
        setWarning(undefined);
        setDraft({});
        notify(`${section.label} saved`);
      } catch (cause) {
        setWarning(undefined);
        // Another session saved it first: what it holds now replaces the edits.
        if (cause instanceof ApiError && cause.code === 'settings_changed_reload_before_saving') {
          setDraft({});
          refresh();
        }
        throw cause;
      }
    },
    { inline: true },
  );
  const element = sectionElement(section.id, reveal);
  const heading = `${element.id}-heading`;
  const row = (field: SettingsField) => (
    <FieldRow
      key={field.id}
      field={field}
      id={`${element.id}-${field.id}`}
      value={holds(field)}
      before={before(field)}
      change={(value) => setDraft((old) => ({ ...old, [field.id]: value }))}
    />
  );
  return (
    <section className="settings-section" aria-labelledby={heading} {...element}>
      <h2 id={heading}>{section.label}</h2>
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
    </section>
  );
}
