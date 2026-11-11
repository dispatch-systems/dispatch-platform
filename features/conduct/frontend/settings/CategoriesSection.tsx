import { useState } from 'react';
import { Plus } from 'lucide-react';
import { DataState } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type { ConductCategory } from '../../api/index.js';
import { restoreCategory } from '../../api/client.js';
import { TierBadge } from '../parts.js';
import { useConduct } from '../useConduct.js';
import { stepDownRule } from './StepDown.js';
import { CategoryDialog } from './CategoryDialog.js';

/** The categories of writeup on Conduct's page of Settings: each in use, then those retired. */
export function CategoriesSection() {
  const { overview, error, refresh, take } = useConduct();
  const [open, setOpen] = useState<ConductCategory | 'new'>();
  const restore = useAction(async (id: string) => take(await restoreCategory(id)), {
    success: 'Category restored',
  });
  return (
    <section className="settings-section conduct-categories" aria-labelledby="conduct-categories">
      <div className="conduct-categories-head">
        <h2 id="conduct-categories">Categories</h2>
        <button type="button" onClick={() => setOpen('new')}>
          <Plus size={16} />
          Add category
        </button>
      </div>
      <DataState data={overview} error={error} retry={refresh}>
        {(overview) => {
          const count = (id: string) => overview.categoryCounts[id] ?? 0;
          return (
            <div className="table-wrap">
              <table className="conduct-table">
                <thead>
                  <tr>
                    <th>Category</th>
                    <th>Starts at</th>
                    <th>Step down</th>
                    <th>Writeups</th>
                    <th>
                      <span className="sr-only">Change</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {overview.categories.map((category) => (
                    <tr
                      key={category.id}
                      className={category.retired ? 'conduct-retired' : undefined}
                    >
                      <td>{category.name}</td>
                      <td>
                        {category.retired ? (
                          <span className="muted">Retired</span>
                        ) : (
                          <TierBadge tier={category.startTier} />
                        )}
                      </td>
                      <td>{stepDownRule(category.stepDown)}</td>
                      <td>{count(category.id)}</td>
                      <td className="cell-end">
                        {category.retired ? (
                          <button
                            type="button"
                            className="text-button"
                            disabled={restore.busy}
                            onClick={() => void restore.run(category.id)}
                          >
                            Restore
                          </button>
                        ) : (
                          <button
                            type="button"
                            className="text-button"
                            onClick={() => setOpen(category)}
                          >
                            Edit
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          );
        }}
      </DataState>
      {open && (
        <CategoryDialog
          category={open === 'new' ? undefined : open}
          close={() => setOpen(undefined)}
          people={overview?.people ?? []}
          saved={take}
        />
      )}
    </section>
  );
}
