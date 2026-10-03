export type { Uniform } from '../../../shared/contracts/generated/Uniform';
export type { UniformVariant } from '../../../shared/contracts/generated/UniformVariant';
export type { UniformFit } from '../../../shared/contracts/generated/UniformFit';
export type { UniformInventory } from '../../../shared/contracts/generated/UniformInventory';
export type { UniformAdjustment } from '../../../shared/contracts/generated/UniformAdjustment';
export type { UniformUpdates } from '../../../shared/contracts/generated/UniformUpdates';
export type { UniformHistory } from '../../../shared/contracts/generated/UniformHistory';
export type { UniformEvent } from '../../../shared/contracts/generated/UniformEvent';
import type { UniformFit } from '../../../shared/contracts/generated/UniformFit';

export interface UniformInput {
  name: string;
  category: string;
  revision?: number;
  variants: { id?: string; fit: UniformFit; size: string }[];
}

export const uniformFits = ['men', 'women', 'unisex'] as const;
export const uniformFitLabels: Record<UniformFit, string> = {
  men: 'Men’s',
  women: 'Women’s',
  unisex: 'Unisex',
};
export const uniformSizePresets = ['XS', 'S', 'M', 'L', 'XL', '2XL', '3XL', '4XL', '5XL', '6XL'];
export const uniformCategoryPresets = ['Tops', 'Bottoms', 'Vests', 'Jackets', 'Hats'];
