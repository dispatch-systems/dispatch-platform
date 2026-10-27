import { Fragment, type ReactNode } from 'react';
import type { Writing } from '../../api/index.js';
import { clock, listTime, preview } from '../shared/format.js';
import { rowKey, waits, type RouteGroup, type RouteRow } from './todays-routes.js';
import { Face, SenderFace } from '../shared/Face.js';
import { PartyFace } from '../conversations/party.js';
import { headingText, NoRoutes } from './headings.js';
import { PaceMark, RouteCode, Stops, WorkPhone } from './RouteFacts.js';
import { typingNote } from '../shared/format.js';
import { Wait } from '../shared/Wait.js';
import './routes-board.css';

// Today's Routes as a board across the page, nothing open: its groups as the list's, each driver a
// line of their route, wave, van, work phone, stops and pace, and what was last written, with how
// long it has waited or when. Choosing one opens it in the list.

export function RoutesBoard({
  tools,
  groups,
  typing,
  timeZone,
  now,
  choose,
}: {
  /** The sections, Search and the view, above the board. */
  tools: ReactNode;
  groups: readonly RouteGroup[];
  typing: readonly Writing[];
  timeZone: string;
  now: number;
  choose: (row: RouteRow) => void;
}) {
  return (
    <section className="routes-board" aria-label="Today's Routes">
      <div className="board-tools">{tools}</div>
      {groups.length === 0 ? (
        <NoRoutes />
      ) : (
        <div className="board-scroll">
          <table>
            <thead>
              <tr>
                <th>Driver</th>
                <th>Route</th>
                <th>Wave</th>
                <th>Van</th>
                <th>Work phone</th>
                <th colSpan={2}>Stops</th>
                <th>Last text</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {groups.map((group) => (
                <Fragment key={group.key}>
                  <tr className="board-group">
                    <th colSpan={9} scope="colgroup">
                      {headingText(group.heading, timeZone)} · {group.rows.length}
                    </th>
                  </tr>
                  {group.rows.map((row) => (
                    <BoardRow
                      key={rowKey(row)}
                      row={row}
                      typing={typing}
                      timeZone={timeZone}
                      now={now}
                      choose={choose}
                    />
                  ))}
                </Fragment>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

/** A line of the board: a driver and their route, or a work phone nobody held. */
function BoardRow({
  row,
  typing,
  timeZone,
  now,
  choose,
}: {
  row: RouteRow;
  typing: readonly Writing[];
  timeZone: string;
  now: number;
  choose: (row: RouteRow) => void;
}) {
  const { conversation } = row;
  const today = row.kind === 'driver' ? row.driver.today : undefined;
  const name = row.kind === 'driver' ? row.driver.person.name : row.conversation.with.name;
  const writing = typing.filter((writer) => writer.conversationId === conversation?.id);
  const last = conversation?.last;
  return (
    <tr className={waits(row) ? 'waits' : undefined} onClick={() => choose(row)}>
      <td>
        <button
          className="board-who"
          onClick={(event) => {
            event.stopPropagation();
            choose(row);
          }}
        >
          {row.kind === 'driver' ? (
            <Face {...row.driver.person} size="message" />
          ) : (
            <PartyFace conversation={row.conversation} size="message" />
          )}
          {name}
        </button>
      </td>
      <td>{today?.route && <RouteCode code={today.route} />}</td>
      <td>{today?.waveAt != null && clock(today.waveAt, timeZone)}</td>
      <td>{today?.van}</td>
      <td>{today?.phone && <WorkPhone number={today.phone} />}</td>
      <td className="board-stops">
        <div>{today && <Stops today={today} />}</div>
      </td>
      <td>{today && <PaceMark pace={today.pace} />}</td>
      <td className="board-last">
        {writing.length > 0 ? (
          <span className="typing-note">{typingNote(writing)}</span>
        ) : (
          last && (
            <>
              {last.sender && <SenderFace sender={last.sender} size="sender" />}
              {preview(last)}
            </>
          )
        )}
      </td>
      <td className="board-when">
        {conversation?.state === 'waiting' && conversation.waitingSince !== null ? (
          <Wait since={conversation.waitingSince} now={now} />
        ) : (
          conversation?.lastAt != null && (
            <time>{listTime(conversation.lastAt, timeZone, now)}</time>
          )
        )}
      </td>
    </tr>
  );
}
