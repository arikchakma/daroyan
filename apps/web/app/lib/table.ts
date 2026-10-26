import {
  columnVisibilityFeature,
  createSortedRowModel,
  rowSelectionFeature,
  rowSortingFeature,
  sortFn_alphanumeric,
  sortFn_basic,
  sortFn_datetime,
  sortFn_text,
  tableFeatures,
} from '@tanstack/react-table';
import type { ReactTable, SortingState, Updater } from '@tanstack/react-table';

export type CellSkeletonShape =
  | 'text'
  | 'lines'
  | 'lines-lg'
  | 'avatar-text'
  | 'avatar-lines'
  | 'avatar-lines-lg'
  | 'icon-lines-lg'
  | 'badge'
  | 'checkbox'
  | 'none';

type DataColumnMeta = {
  width?: number;
  align?: 'left' | 'right';
  hideBelow?: 'sm' | 'md' | 'lg' | 'xl';
  skeleton?: CellSkeletonShape;
};

export const dataTableFeatures = tableFeatures({
  columnVisibilityFeature,
  rowSelectionFeature,
  rowSortingFeature,
  sortedRowModel: createSortedRowModel(),
  sortFns: {
    alphanumeric: sortFn_alphanumeric,
    basic: sortFn_basic,
    datetime: sortFn_datetime,
    text: sortFn_text,
  },
  columnMeta: {} as DataColumnMeta,
});

export type DataTableFeatures = typeof dataTableFeatures;

export type DataTableInstance<TData extends object> = ReactTable<
  DataTableFeatures,
  TData
>;

export function resolveUpdater<T>(updater: Updater<T>, previous: T) {
  return typeof updater === 'function'
    ? (updater as (old: T) => T)(previous)
    : updater;
}

type ToSortingOptions = {
  sort: string;
  direction: 'asc' | 'desc';
};

export function toSorting(options: ToSortingOptions): SortingState {
  const { sort, direction } = options;

  return [{ id: sort, desc: direction === 'desc' }];
}
