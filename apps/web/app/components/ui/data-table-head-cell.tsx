import type { Column } from '@tanstack/react-table';
import type { ReactNode } from 'react';

import type { DataTableFeatures } from '~/lib/table';
import { cn } from '~/utils/classname';

type DataTableHeadCellProps<TData extends object, TValue> = {
  column: Column<DataTableFeatures, TData, TValue>;
  variant: 'page' | 'card';
  children: ReactNode;
};

export function DataTableHeadCell<TData extends object, TValue>(
  props: DataTableHeadCellProps<TData, TValue>
) {
  const { column, variant, children } = props;

  const meta = column.columnDef.meta;
  const sorted = column.getIsSorted();

  return (
    <th
      scope="col"
      aria-sort={
        sorted === 'asc'
          ? 'ascending'
          : sorted === 'desc'
            ? 'descending'
            : undefined
      }
      style={{ width: meta?.width }}
      className={cn(
        'h-7.5 bg-surface-row px-2 text-left font-medium text-text-secondary',
        variant === 'page' &&
          'first:rounded-l-sm first:pl-3 last:rounded-r-sm last:pr-3',
        variant === 'card' && 'first:pl-4 last:pr-4',
        meta?.align === 'right' && 'text-right',
        meta?.hideBelow === 'sm' && 'max-sm:hidden',
        meta?.hideBelow === 'md' && 'max-md:hidden',
        meta?.hideBelow === 'lg' && 'max-lg:hidden',
        meta?.hideBelow === 'xl' && 'max-xl:hidden'
      )}
    >
      {children}
    </th>
  );
}
