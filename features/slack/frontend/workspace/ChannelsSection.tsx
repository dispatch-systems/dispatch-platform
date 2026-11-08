import { useRef, useState } from 'react';
import { Hash, Lock } from 'lucide-react';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { DataState, ErrorBox, SearchInput } from '../../../../core/shell/frontend/ui/index.js';
import { setSlackChannel, useSlackChannels, type SlackChannel } from '../../api/client.js';

// Slack's own section of a workspace's settings: the channels Dispatch is in, and those it may
// join. Joining a public channel, or leaving any, happens in Slack at once, and the channel moves
// to its list as it is asked, one asked after another saved in turn; one that fails moves back.
// A private one shows once someone invites Dispatch to it in Slack, and goes once it leaves.

export function ChannelsSection({ team }: { team: string }) {
  const channels = useSlackChannels(team);
  // Each channel as Slack last answered joining or leaving it, over what was read; and, while
  // asked and not yet answered, whether the latest asked joins it, with which asking that was.
  const [answered, setAnswered] = useState<Record<string, SlackChannel>>({});
  const [requested, setRequested] = useState<Record<string, { member: boolean; asking: number }>>(
    {},
  );
  const [query, setQuery] = useState('');
  const queue = useRef(Promise.resolve());
  const askings = useRef(0);
  const saving = useAction(
    async (channel: string, member: boolean) => {
      const after = await setSlackChannel(team, channel, member);
      setAnswered((was) => ({ ...was, [after.id]: after }));
    },
    { inline: true },
  );
  // Each asking saves after those before it; once answered, or failed, it shows what Slack last
  // answered unless another was asked since.
  const set = (channel: SlackChannel, member: boolean) => {
    askings.current += 1;
    const asking = askings.current;
    setRequested((was) => ({ ...was, [channel.id]: { member, asking } }));
    queue.current = queue.current.then(async () => {
      await saving.run(channel.id, member);
      setRequested((was) => {
        if (was[channel.id]?.asking !== asking) return was;
        const { [channel.id]: _done, ...rest } = was;
        return rest;
      });
    });
  };
  const asked = query.trim().replace(/^#/, '').toLowerCase();
  return (
    <DataState data={channels.data} error={channels.error} retry={channels.refresh}>
      {(data) => {
        const shown = data.channels
          .map((channel) => {
            const known = answered[channel.id] ?? channel;
            const latest = requested[channel.id];
            return latest ? { ...known, member: latest.member } : known;
          })
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
            <ErrorBox message={saving.error} />
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
                          onClick={() => set(channel, !member)}
                        >
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
