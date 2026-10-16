import { useState } from 'react';
import { Plus } from 'lucide-react';
import type { ConnectionPiece, SettingsContext } from '../../shell/frontend/runtime/slots.js';
import { Popover } from '../../shell/frontend/ui/index.js';

/**
 * New on DSP Connections: the kinds of account the DSP may add another of, as the cards that
 * show them offer them, and what the one chosen opens.
 */
export function NewConnection({
  pieces,
  context,
}: {
  /** The cards with an entry in New, in their order. */
  pieces: readonly ConnectionPiece[];
  context: SettingsContext;
}) {
  const [adding, setAdding] = useState<ConnectionPiece>();
  return (
    <>
      <Popover
        className="row-menu connections-new"
        label="New"
        trigger={
          <>
            <Plus size={16} />
            New
          </>
        }
        anchored
      >
        {pieces.map((piece) => (
          <button key={piece.id} onClick={() => setAdding(piece)}>
            {piece.adds!.icon()}
            {piece.adds!.label}
          </button>
        ))}
      </Popover>
      {adding?.adds!.render({ ...context, close: () => setAdding(undefined) })}
    </>
  );
}
