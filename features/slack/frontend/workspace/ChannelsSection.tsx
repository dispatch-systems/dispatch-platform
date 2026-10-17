import { useState } from 'react';
import { Hash, LoaderCircle, Lock } from 'lucide-react';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { DataState, ErrorBox, SearchInput } from '../../../../core/shell/frontend/ui/index.js';
import { setSlackChannel, useSlackChannels, type SlackChannel } from '../../api/client.js';

// Slack's own section of a workspace's settings: the channels Dispatch is in, and those it may
// join. Joining a public channel, or leaving any, happens in Slack at once. A private one shows
// once someone invites Dispatch to it in Slack, and goes once it leaves.

export function ChannelsSection({ team }: { team: string }) {
  const channels = useSlackChannels(team);
  // Each channel as joining or leaving it left it, over what was read.
  const [changed, setChanged] = useState<Record<string, SlackChannel>>({});
  const [changing, setChanging] = useState<string>();
  const [query, setQuery] = useState('');
  const set = useAction(
    async (channel: SlackChannel, member: boolean) => {
      setChanging(channel.id);
      try {
        const after = await setSlackChannel(team, channel.id, member);
        setChanged((was) => ({ ...was, [after.id]: after }));
      } finally {
        setChanging(undefined);
      }
    },
    { inline: true },
  );
  const asked = query.trim().replace(/^#/, '').toLowerCase();
  return (
    <DataState data={channels.data} error={channels.error} retry={channels.refresh}>
      {(data) => {
        const shown = data.channels
          .map((channel) => changed[channel.id] ?? channel)
          .filter((channel) => !channel.private || channel.member)
          .filter((channel) => channel.name.toLowerCase().includes(asked));
        const group = (member: boolean) => shown.filter((channel) => channel.member === member);
        return (
          <div className="slack-channels">
            <SearchInput
              label="Search channels"
              placeholder="Search"
              value={query}
              onChange={setQuery}
            />
            <ErrorBox message={set.error} />
            {[true, false].map((member) => {
              const listed = group(member);
              if (!listed.length) return null;
              const heading = member ? 'Joined' : 'Not joined';
              return (
                <div className="slack-channel-group" key={heading}>
                  <h3>
                    {heading}
                    <span className="slack-count">{listed.length}</span>
                  </h3>
                  <ul aria-label={heading}>
                    {listed.map((channel) => (
                      <li key={channel.id}>
                        <span className="slack-channel-name">
                          {channel.private ? <Lock size={15} /> : <Hash size={15} />}
                          {channel.name}
                          {channel.private && <span className="slack-private">Private</span>}
                        </span>
                        <button
                          className={member ? undefined : 'primary'}
                          aria-label={`${member ? 'Leave' : 'Join'} ${channel.name}`}
                          disabled={changing !== undefined}
                          onClick={() => void set.run(channel, !member)}
                        >
                          {changing === channel.id && <LoaderCircle className="spin" size={14} />}
                          {member ? 'Leave' : 'Join'}
                        </button>
                      </li>
                    ))}
                  </ul>
                </div>
              );
            })}
          </div>
        );
      }}
    </DataState>
  );
}
