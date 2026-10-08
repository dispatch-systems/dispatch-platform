import { useState, type ReactNode } from 'react';
import { CircleCheck, Mail, TriangleAlert } from 'lucide-react';
import type { DspView } from '../../../core/accounts/api/index.js';
import { time } from '../../../core/shell/frontend/lib/format.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { ConfirmDialog, DataState, Modal } from '../../../core/shell/frontend/ui/index.js';
import {
  disconnectGoogle,
  emailAgain,
  removeShare,
  useDocumentsTeam,
  type DocumentsConnection,
  type DocumentsTeam,
  type TeamPerson,
} from '../api/client.js';
import { GoogleLogo } from './GoogleLogo.js';
import { Initials, editedInline } from './items.js';
import { useSignIn } from './signIn.js';

// Who on the team edits the DSP's Documents in Google, for those who manage it: the account
// that holds the folder and how full it is, who can't edit yet and why, who else the folder
// is shared with, and connecting it again or letting it go.

const gigabytes = (bytes: number) => `${(bytes / 1_000_000_000).toFixed(1)} GB`;
/** Google's reasons for refusing a share, as an owner can act on them. */
const refusals: Record<string, string> = {
  shareOutNotPermitted: 'The Google Workspace only shares inside itself.',
  shareInNotPermitted: "Their Google Workspace doesn't take shares from outside it.",
};

export function TeamPanel({
  view,
  connection,
  close,
  disconnected,
}: {
  view: DspView;
  connection: DocumentsConnection;
  close: () => void;
  disconnected: () => void;
}) {
  const team = useDocumentsTeam();
  const [shown, setShown] = useState<DocumentsTeam>();
  const signIn = useSignIn();
  const [confirming, setConfirming] = useState(false);
  const disconnect = useAction(
    async () => {
      await disconnectGoogle();
      setConfirming(false);
      close();
      disconnected();
    },
    { success: 'Google disconnected' },
  );
  const emailing = useAction(
    async (person: TeamPerson) => setShown(await emailAgain(person.userId)),
    {
      success: (person) => `Emailed ${person.name} again`,
    },
  );
  const removing = useAction(async (share: string) => setShown(await removeShare(share)), {
    success: 'Removed from the folder',
  });
  const holder =
    connection.accountKind === 'workspace' ? 'Google Workspace account' : 'Google account';
  return (
    <Modal variant="sheet" title="Team access" onClose={close}>
      <div className="documents-team">
        <p className="muted">
          Everyone at {view.dsp.name} who uses Documents can open it here. Dispatch shares the
          Google folder with each of them, and takes it back when they leave your team.
        </p>
        <section className="documents-account-card">
          <GoogleLogo size={22} />
          <div>
            <strong>{connection.accountEmail}</strong>
            <small>
              {holder} that holds the {connection.folderName} folder
            </small>
          </div>
          <button disabled={signIn.busy} onClick={() => void signIn.run()}>
            Reconnect
          </button>
        </section>
        <DataState data={shown ?? team.data} error={team.error} retry={team.refresh}>
          {(data) => (
            <>
              <Storage team={data} />
              <People
                title="Need a Google account"
                icon={<Mail size={16} />}
                people={data.people.filter((person) => person.state === 'needs_account')}
                about="They can see your team's files here, but can't edit Docs and Sheets in Google until they link a Google account. Dispatch emailed each of them how."
                action={(person) => (
                  <button disabled={emailing.busy} onClick={() => void emailing.run(person)}>
                    Email again
                  </button>
                )}
                note={(person) =>
                  person.emailedAt
                    ? `Emailed ${editedInline(person.emailedAt, view.dsp.timezone)}`
                    : ''
                }
              />
              <People
                title="Google can't share with them"
                icon={<TriangleAlert size={16} />}
                people={data.people.filter((person) => person.state === 'refused')}
                about="Google refused to share the folder with them."
                note={(person) =>
                  refusals[person.refusal ?? ''] ?? `Google said ${person.refusal ?? 'no'}.`
                }
              />
              {data.outsiders.length > 0 && (
                <section className="documents-team-group alert">
                  <h3>
                    <TriangleAlert size={16} />
                    In the Google folder but not on your team
                    <span>{data.outsiders.length}</span>
                  </h3>
                  <p className="muted">Someone shared the folder with them in Google Drive.</p>
                  {data.outsiders.map((outsider) => (
                    <div className="documents-team-row" key={outsider.shareId}>
                      <span className="documents-initials documents-tone-slate">?</span>
                      <div>
                        <strong>{outsider.email}</strong>
                      </div>
                      <button
                        className="danger"
                        disabled={removing.busy}
                        onClick={() => void removing.run(outsider.shareId)}
                      >
                        Remove
                      </button>
                    </div>
                  ))}
                </section>
              )}
              <People
                title="Can edit"
                icon={<CircleCheck size={16} />}
                people={data.people.filter((person) => person.state === 'shared')}
                tone="ok"
                action={(person) => person.owner && <span className="documents-tag">Owner</span>}
              />
            </>
          )}
        </DataState>
        <footer className="documents-account-foot">
          <span className="muted">
            Connected {time(connection.connectedAt, view.dsp.timezone)}
            {connection.connectedBy ? ` by ${connection.connectedBy}` : ''}
          </span>
          <button className="documents-disconnect" onClick={() => setConfirming(true)}>
            Disconnect Google
          </button>
        </footer>
      </div>
      {confirming && (
        <ConfirmDialog
          title="Disconnect Google?"
          confirm="Disconnect"
          tone="danger"
          busy={disconnect.busy}
          onConfirm={() => void disconnect.run()}
          onCancel={() => setConfirming(false)}
        >
          Your team stops seeing Documents here, and Dispatch's access to {connection.accountEmail}{' '}
          is removed. {connection.folderName} and everything in it stay in that account's Google
          Drive, shared as it is.
        </ConfirmDialog>
      )}
    </Modal>
  );
}

/** How full the account is, with a warning once it is nearly full. */
function Storage({ team }: { team: DocumentsTeam }) {
  if (team.storageLimit === null)
    return <p className="documents-storage muted">{gigabytes(team.storageUsed)} of storage used</p>;
  const share = Math.min(100, Math.round((team.storageUsed / team.storageLimit) * 100));
  return (
    <div className="documents-storage">
      <div>
        <span>
          <strong>{gigabytes(team.storageUsed)}</strong> of {gigabytes(team.storageLimit)} used
        </span>
        <span>{share}%</span>
      </div>
      <div className={`documents-meter${share >= 90 ? ' full' : ''}`} role="presentation">
        <span style={{ width: `${share}%` }} />
      </div>
      {share >= 90 && (
        <p className="documents-storage-warning">
          <TriangleAlert size={15} />
          This account is almost full. New files can't be added once it is. Add storage to it with
          Google One, or delete files you no longer need.
        </p>
      )}
    </div>
  );
}

function People({
  title,
  icon,
  people,
  about,
  tone,
  action,
  note,
}: {
  title: string;
  icon: ReactNode;
  people: TeamPerson[];
  about?: string;
  tone?: 'ok';
  action?: (person: TeamPerson) => ReactNode;
  note?: (person: TeamPerson) => string;
}) {
  if (!people.length) return null;
  return (
    <section className={`documents-team-group${tone === 'ok' ? ' ok' : ''}`}>
      <h3>
        {icon}
        {title}
        <span>{people.length}</span>
      </h3>
      {about && <p className="muted">{about}</p>}
      {people.map((person) => (
        <div className="documents-team-row" key={person.userId}>
          <Initials name={person.name} />
          <div>
            <strong>{person.name}</strong>
            <small>
              {person.email}
              {note?.(person) ? ` · ${note(person)}` : ''}
            </small>
          </div>
          {action?.(person)}
        </div>
      ))}
    </section>
  );
}
