import { SearchInput } from '../../../core/shell/frontend/ui/index.js';
import type { ConductCategory } from '../api/index.js';
import { type HistoryFilters } from '../api/client.js';
import { tierShort, tiers } from './tiers.js';

export function useConductFilters(
  categories: ConductCategory[],
  filters: HistoryFilters,
  change: (filters: HistoryFilters) => void,
) {
  const { search, category, tier } = filters;
  return {
    search,
    category,
    tier,
    controls: (
      <>
        <SearchInput
          label="Search drivers"
          placeholder="Search drivers"
          value={search}
          onChange={(search) => change({ ...filters, search })}
        />
        <select
          aria-label="Category"
          value={category}
          onChange={(e) => change({ ...filters, category: e.target.value })}
        >
          <option value="">All categories</option>
          {categories.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </select>
        <select
          aria-label="Tier"
          value={tier}
          onChange={(e) => change({ ...filters, tier: e.target.value })}
        >
          <option value="">All tiers</option>
          {tiers.map((t) => (
            <option key={t} value={t}>
              {tierShort[t]}
            </option>
          ))}
        </select>
      </>
    ),
  };
}
