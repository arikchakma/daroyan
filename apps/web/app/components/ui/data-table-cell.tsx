import type { Column } from '@tanstack/react-table';
import type { ReactNode } from 'react';

import type { DataTableFeatures } from '~/lib/table';
import { cn } from '~/utils/classname';

type DataTableCellProps<TData extends object, TValue> = {
  column: Column<DataTableFeatures, TData, TValue>;
  variant: 'page' | 'card';
  size: 'sm' | 'md' | 'lg';
  children: ReactNode;
};

export function DataTableCell<TData extends object, TValue>(
  props: DataTableCellProps<TData, TValue>
) {
  const { column, variant, size, children } = props;

  const meta = column.columnDef.meta;

  return (
    <td
      className={cn(
        'px-2 text-text-primary',
        size === 'sm' && 'h-8',
        size === 'md' && 'h-10',
        size === 'lg' && 'h-11.5',
        variant === 'page' && 'first:pl-3 last:pr-3',
        variant === 'card' && 'first:pl-4 last:pr-4',
        meta?.align === 'right' && 'text-right',
        meta?.hideBelow === 'sm' && 'max-sm:hidden',
        meta?.hideBelow === 'md' && 'max-md:hidden',
        meta?.hideBelow === 'lg' && 'max-lg:hidden',
        meta?.hideBelow === 'xl' && 'max-xl:hidden'
      )}
    >
      {children}
    </td>
  );
}
