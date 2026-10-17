import { Hash, Lock } from 'lucide-react';
import type { SlackChannel } from '../../api/client.js';

// Choosing one of the channels Dispatch is in, each by its name, those taken already shown as
// such and not chosen; or, while it's in none, going to Channels to join one.

export function ChannelChoice({
  channels,
  chosen,
  choose,
  taken = new Set(),
}: {
  channels: readonly SlackChannel[];
  chosen: string | undefined;
  choose: (channel: string) => void;
  /** The channels that can't be chosen, by ID. */
  taken?: ReadonlySet<string>;
}) {
  return (
    <fieldset>
      <legend>Channel</legend>
      <ul className="slack-choices">
        {channels.map((each) => {
          const isTaken = taken.has(each.id);
          return (
            <li key={each.id}>
              <label className={isTaken ? 'taken' : undefined}>
                <input
                  type="radio"
                  name="channel"
                  checked={chosen === each.id}
                  disabled={isTaken}
                  onChange={() => choose(each.id)}
                />
                {each.private ? <Lock size={14} /> : <Hash size={14} />}
                {each.name}
                {isTaken && <span>Linked</span>}
              </label>
            </li>
          );
        })}
      </ul>
    </fieldset>
  );
}

/** While Dispatch is in no channel: Channels, to join one. */
export function NoChannels({
  open,
  close,
}: {
  /** Opens another of the workspace's sections, by its id. */
  open: (section: string) => void;
  close: () => void;
}) {
  return (
    <div className="slack-link-none">
      <p>Dispatch isn't in any channels yet.</p>
      <div className="form-actions">
        <button onClick={close}>Cancel</button>
        <button
          className="primary"
          onClick={() => {
            close();
            open('channels');
          }}
        >
          Open Channels
        </button>
      </div>
    </div>
  );
}
