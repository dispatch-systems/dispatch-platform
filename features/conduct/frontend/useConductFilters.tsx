import { useState } from 'react';
import { SearchInput } from '../../../core/shell/frontend/ui/index.js';
import type { ConductCategory, ConductTier } from '../api/index.js';
import { tierShort, tiers } from './tiers.js';

export function useConductFilters(categories: ConductCategory[]) {
  const [search, setSearch] = useState('');
  const [category, setCategory] = useState('');
  const [tier, setTier] = useState<ConductTier | ''>('');
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
          onChange={setSearch}
        />
        <select
          aria-label="Category"
          value={category}
          onChange={(e) => setCategory(e.target.value)}
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
          onChange={(e) => setTier(e.target.value as ConductTier | '')}
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
