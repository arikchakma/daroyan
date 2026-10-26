import { useTable } from '@tanstack/react-table';
import type { TableOptions } from '@tanstack/react-table';

import { dataTableFeatures } from '~/lib/table';
import type { DataTableFeatures } from '~/lib/table';

type UseDataTableOptions<TData extends { id: string }> = Omit<
  TableOptions<DataTableFeatures, TData>,
  'features'
>;

export function useDataTable<TData extends { id: string }>(
  options: UseDataTableOptions<TData>
) {
  return useTable({
    features: dataTableFeatures,
    getRowId: (row) => row.id,
    enableMultiSort: false,
    enableSortingRemoval: false,
    ...options,
  });
}
