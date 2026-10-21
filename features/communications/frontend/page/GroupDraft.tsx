import { useCallback, useEffect, useId, useMemo, useState } from 'react';
import { ArrowLeft, Check, X } from 'lucide-react';
import type {
  CommunicationsWorkspace,
  Correspondent,
  Group,
  GroupChoice,
  GroupChoices,
  Reachable,
} from '../../api/index.js';
import { createGroup, getContacts, getGroupChoices } from '../../api/client.js';
import { ErrorBox, Loading } from '../../../../core/shell/frontend/ui/index.js';
import { messageOf } from '../../../../core/shell/frontend/lib/errors.js';
import {
  choiceKey,
  choiceName,
  faces,
  groupName,
  keyOf,
  peopleText,
  recipients,
  type Pick,
} from '../groups.js';
import { Composer } from './Composer.js';
import { Face } from './Face.js';
import { Name, roleText } from './parts.js';
import { useFound } from './useFound.js';
import { AllWorkspacesMark, WorkspaceMark } from './WorkspaceMark.js';

// Messaging a group: whom to, picked in To by a name, someone's or a group's, or from the groups
// below it, each a chip; then the message, sent to each of them once. Today's groups, those on a
// route and each wave; Roles; and with two or more workspaces, Everyone and each workspace's.

/** What the list's draft row shows of what's being written: whom to, and how many. */
export type Draft = { name: string; people: number };

export function GroupDraft({
  workspaces,
  timeZone,
  back,
  drafted,
  sent,
}: {
  /** The workspaces Communications works in. */
  workspaces: readonly CommunicationsWorkspace[];
  timeZone: string;
  back: () => void;
  /** Tells the list's draft row whom it's to. */
  drafted: (draft: Draft) => void;
  /** Opens the group once it's sent. */
  sent: (group: Group) => void;
}) {
  const [choices, setChoices] = useState<GroupChoices>();
  const [failed, setFailed] = useState('');
  useEffect(() => {
    getGroupChoices().then(setChoices, (cause) => setFailed(messageOf(cause)));
  }, []);
  const [picks, setPicks] = useState<Pick[]>([]);
  const people = useMemo(
    () => new Map((choices?.people ?? []).map((person) => [person.key, person])),
    [choices],
  );
  const named = useCallback(
    (pick: Pick) =>
      pick.kind === 'person'
        ? pick.contact.person.name
        : choiceName(pick.choice, workspaces, timeZone),
    [workspaces, timeZone],
  );
  const to = useMemo(() => recipients(picks, choices?.people ?? []), [picks, choices]);
  const name = picks.length ? groupName(picks, named) : '';
  useEffect(() => drafted({ name, people: to.length }), [drafted, name, to.length]);
  const add = (pick: Pick) =>
    setPicks((now) => (now.some((each) => each.key === pick.key) ? now : [...now, pick]));
  const remove = (key: string) => setPicks((now) => now.filter((each) => each.key !== key));
  const toggle = (choice: GroupChoice) => {
    const key = choiceKey(choice);
    if (picks.some((pick) => pick.key === key)) remove(key);
    else add({ kind: 'group', key, choice });
  };
  const sections: [string, GroupChoice[]][] = choices
    ? [
        ['Today', choices.today],
        ['Roles', choices.roles],
        ['Workspaces', choices.workspaces],
      ]
    : [];
  return (
    <section className="pane-thread group-draft" aria-label="New message">
      <ToLine
        picks={picks}
        choices={choices}
        people={people}
        named={named}
        add={add}
        remove={remove}
        back={back}
      />
      <div className="group-cards">
        <ErrorBox message={failed} />
        {!choices && !failed && <Loading />}
        {sections.map(
          ([title, listed]) =>
            listed.length > 0 && (
              <section key={title}>
                <h3>{title}</h3>
                <div className="cards">
                  {listed.map((choice) => {
                    const key = choiceKey(choice);
                    const on = picks.some((pick) => pick.key === key);
                    return (
                      <button
                        key={key}
                        className={on ? 'pick on' : 'pick'}
                        aria-pressed={on}
                        onClick={() => toggle(choice)}
                      >
                        <Mark choice={choice} people={people} workspaces={workspaces} />
                        <span className="pick-body">
                          <strong>{choiceName(choice, workspaces, timeZone)}</strong>
                          <small>{peopleText(choice.people.length)}</small>
                        </span>
                        {on && (
                          <span className="tick" aria-hidden="true">
                            <Check size={14} />
                          </span>
                        )}
                      </button>
                    );
                  })}
                </div>
              </section>
            ),
        )}
      </div>
      <Composer
        placeholder="Message"
        writable
        ready={to.length > 0}
        label={to.length ? `Send to ${to.length}` : undefined}
        send={async (text, photos) => sent(await createGroup(name, to, text, photos))}
      />
    </section>
  );
}

/**
 * Whom to: each pick a chip, its faces, its name and, for a group, how many it holds, with a way
 * to take it out; then a name typed, which finds groups by their names and people however their
 * names are spelt, the arrow keys moving through them and Enter picking one. Backspace in an
 * empty field takes out the last chip.
 */
function ToLine({
  picks,
  choices,
  people,
  named,
  add,
  remove,
  back,
}: {
  picks: readonly Pick[];
  choices?: GroupChoices;
  people: ReadonlyMap<string, Reachable>;
  named: (pick: Pick) => string;
  add: (pick: Pick) => void;
  remove: (key: string) => void;
  back: () => void;
}) {
  const [typed, setTyped] = useState('');
  const [active, setActive] = useState(0);
  const list = useId();
  const field = useId();
  const name = typed.trim();
  const read = useCallback(() => getContacts(name).then((answer) => answer.contacts), [name]);
  const { found: contacts } = useFound(name ? read : undefined, 'found', 0, 150);
  const groups: Pick[] = name
    ? [...(choices?.today ?? []), ...(choices?.roles ?? []), ...(choices?.workspaces ?? [])]
        .map((choice): Pick => ({ kind: 'group', key: choiceKey(choice), choice }))
        .filter((pick) => named(pick).toLocaleLowerCase().includes(name.toLocaleLowerCase()))
    : [];
  // Someone with accounts in several workspaces is found once, by the closest.
  const someone = (contacts ?? [])
    .map((contact): Pick => ({ kind: 'person', key: keyOf(contact), contact }))
    .filter((pick, at, all) => all.findIndex((each) => each.key === pick.key) === at);
  const found: Pick[] = [...groups, ...someone];
  // Each answer is gone through from its first.
  useEffect(() => setActive(0), [contacts, name]);
  useEffect(() => {
    document.getElementById(`${list}-${active}`)?.scrollIntoView({ block: 'nearest' });
  }, [list, active]);
  const choose = (pick: Pick | undefined) => {
    if (!pick) return;
    add(pick);
    setTyped('');
  };
  const option = (at: number) => `${list}-${at}`;
  return (
    <div className="to-line">
      <button className="icon-button back" aria-label="Back" onClick={back}>
        <ArrowLeft size={18} />
      </button>
      <label className="to" htmlFor={field}>
        To
      </label>
      <div className="to-field">
        {picks.map((pick) => (
          <span key={pick.key} className="chip">
            <Faces
              faces={pick.kind === 'person' ? [pick.contact.person] : faces(pick.choice, people)}
            />
            {named(pick)}
            {pick.kind === 'group' && <span className="n">{pick.choice.people.length}</span>}
            <button
              className="chip-remove"
              aria-label={`Remove ${named(pick)}`}
              onClick={() => remove(pick.key)}
            >
              <X size={14} />
            </button>
          </span>
        ))}
        <input
          id={field}
          role="combobox"
          aria-label="To"
          aria-expanded={found.length > 0}
          aria-controls={list}
          aria-activedescendant={found.length ? option(active) : undefined}
          aria-autocomplete="list"
          placeholder={picks.length ? '' : 'Name or group'}
          autoFocus
          value={typed}
          onChange={(event) => setTyped(event.target.value)}
          onKeyDown={(event) => {
            const count = found.length;
            if (event.key === 'ArrowDown' && count) setActive((active + 1) % count);
            else if (event.key === 'ArrowUp' && count) setActive((active - 1 + count) % count);
            else if (event.key === 'Enter') choose(found[active]);
            else if (event.key === 'Escape' && typed) setTyped('');
            else if (event.key === 'Backspace' && !typed && picks.length) remove(picks.at(-1)!.key);
            else return;
            event.preventDefault();
          }}
        />
      </div>
      {name && found.length > 0 && (
        <ul id={list} role="listbox" aria-label="Found" className="to-found">
          {found.map((pick, at) => (
            <li
              key={pick.key}
              id={option(at)}
              role="option"
              aria-selected={at === active}
              className={at === active ? 'result active' : 'result'}
              onMouseEnter={() => setActive(at)}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => choose(pick)}
            >
              {pick.kind === 'person' ? (
                <>
                  <Face {...pick.contact.person} size="contact" />
                  <span className="result-body">
                    <strong>
                      <Name name={pick.contact.person.name} matched={pick.contact.matched} />
                    </strong>
                    <small>{roleText(pick.contact.person)}</small>
                  </span>
                </>
              ) : (
                <>
                  <Faces faces={faces(pick.choice, people)} />
                  <span className="result-body">
                    <strong>{named(pick)}</strong>
                    <small>{peopleText(pick.choice.people.length)}</small>
                  </span>
                </>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** A few people's faces, one over the next. */
function Faces({ faces }: { faces: readonly Correspondent[] }) {
  return (
    <span className="faces" aria-hidden="true">
      {faces.map((person) => (
        <Face key={person.userId} {...person} size="sender" />
      ))}
    </span>
  );
}

/** What shows a group to choose: a few of its people, or a workspace's mark. */
function Mark({
  choice,
  people,
  workspaces,
}: {
  choice: GroupChoice;
  people: ReadonlyMap<string, Reachable>;
  workspaces: readonly CommunicationsWorkspace[];
}) {
  if (choice.kind === 'everyone') return <AllWorkspacesMark />;
  const workspace =
    choice.kind === 'workspace'
      ? workspaces.find((each) => each.teamId === choice.teamId)
      : undefined;
  if (workspace) return <WorkspaceMark workspace={workspace} />;
  return <Faces faces={faces(choice, people)} />;
}
