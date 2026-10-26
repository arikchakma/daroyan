import { createContext } from 'react';

import type {
  EditOptions,
  FilterEditing,
  SearchFilterField,
} from '~/utils/filter';

export type TokenContextValue = {
  fields: SearchFilterField[];
  editing: FilterEditing | null;
  onEdit: (options: EditOptions) => void;
  onRemove: (key: string) => void;
};

export const TokenContext = createContext<TokenContextValue | null>(null);
