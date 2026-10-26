import type { Column } from '@tanstack/react-table';

import { Icon } from '~/components/icons/icon';
import type { DataTableFeatures } from '~/lib/table';
import { cn } from '~/utils/classname';

type SortableHeaderProps<TData extends object, TValue> = {
  column: Column<DataTableFeatures, TData, TValue>;
  label: string;
};

export function SortableHeader<TData extends object, TValue>(
  props: SortableHeaderProps<TData, TValue>
) {
  const { column, label } = props;

  const sorted = column.getIsSorted();
  const isRight = column.columnDef.meta?.align === 'right';

  if (!column.getCanSort()) {
    return label;
  }

  return (
    <button
      type="button"
      onClick={() =>
        column.toggleSorting(sorted ? sorted === 'asc' : undefined)
      }
      className={cn(
        'group/sort flex h-7.5 max-w-full items-center gap-1 outline-none focus-visible:underline',
        isRight && 'ml-auto flex-row-reverse',
        sorted && 'text-text-primary'
      )}
    >
      <span className="truncate">{label}</span>
      <Icon
        name={sorted === 'desc' ? 'arrow-down' : 'arrow-up'}
        className={cn(
          'size-3 shrink-0',
          !sorted &&
            'hidden text-text-tertiary group-hover/sort:block group-focus-visible/sort:block'
        )}
      />
    </button>
  );
}
