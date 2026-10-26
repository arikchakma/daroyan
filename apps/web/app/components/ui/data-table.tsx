import type { ReactNode } from 'react';

import { CellSkeleton } from '~/components/ui/cell-skeleton';
import { DataTableCell } from '~/components/ui/data-table-cell';
import { DataTableHeadCell } from '~/components/ui/data-table-head-cell';
import type { DataTableInstance } from '~/lib/table';
import { cn } from '~/utils/classname';

const SKELETON_ROWS = 12;

type DataTableProps<TData extends object> = {
  table: DataTableInstance<TData>;
  label: string;
  variant?: 'page' | 'card';
  size?: 'sm' | 'md' | 'lg';
  isLoading?: boolean;
  isAllSelected?: boolean;
  bulkBar?: ReactNode;
};

export function DataTable<TData extends object>(props: DataTableProps<TData>) {
  const {
    table,
    label,
    variant = 'page',
    size = 'md',
    isLoading = false,
    isAllSelected = false,
    bulkBar,
  } = props;

  const rows = table.getRowModel().rows;
  const columns = table.getVisibleLeafColumns();
  const isRowSelected = (index: number) =>
    isAllSelected || (rows[index]?.getIsSelected() ?? false);
  const hasSelection =
    isAllSelected || Object.keys(table.state.rowSelection ?? {}).length > 0;
  const isBulkBarShown = hasSelection && !!bulkBar;

  return (
    <table
      aria-label={label}
      className="w-full table-fixed border-separate border-spacing-0 text-caption"
    >
      <thead
        className={cn(
          variant === 'page' && 'sticky-head bg-surface-default',
          variant === 'card' && 'relative'
        )}
      >
        {table.getHeaderGroups().map((group) => (
          <tr key={group.id}>
            {group.headers.map((header, index) => (
              <DataTableHeadCell
                key={header.id}
                column={header.column}
                variant={variant}
              >
                {header.isPlaceholder ? null : isLoading &&
                  header.column.columnDef.meta?.skeleton !== 'checkbox' &&
                  header.column.columnDef.meta?.skeleton !== 'none' ? (
                  <CellSkeleton
                    shape="label"
                    align={header.column.columnDef.meta?.align}
                    seed={index}
                  />
                ) : (
                  <div
                    inert={isBulkBarShown && index > 0}
                    className={cn(isBulkBarShown && index > 0 && 'invisible')}
                  >
                    <table.FlexRender header={header} />
                  </div>
                )}
                {isBulkBarShown && index === 0 && (
                  <div
                    className={cn(
                      'absolute inset-y-0 left-11 flex items-center',
                      variant === 'page' && 'right-3',
                      variant === 'card' && 'right-4'
                    )}
                  >
                    {bulkBar}
                  </div>
                )}
              </DataTableHeadCell>
            ))}
          </tr>
        ))}
      </thead>
      <tbody>
        {isLoading
          ? Array.from({ length: SKELETON_ROWS }, (_, rowIndex) => (
              <tr
                key={rowIndex}
                aria-hidden="true"
                className="[&:not(:first-child)>td]:border-border-row [&>td]:border-t [&>td]:border-transparent"
              >
                {columns.map((column, index) => (
                  <DataTableCell
                    key={column.id}
                    column={column}
                    variant={variant}
                    size={size}
                  >
                    <CellSkeleton
                      shape={column.columnDef.meta?.skeleton ?? 'text'}
                      align={column.columnDef.meta?.align}
                      seed={index + rowIndex}
                    />
                  </DataTableCell>
                ))}
              </tr>
            ))
          : rows.map((row, rowIndex) => {
              const isSelected = isRowSelected(rowIndex);

              return (
                <tr
                  key={row.id}
                  data-selected={isSelected || undefined}
                  className={cn(
                    'group relative [&>td]:border-t [&>td]:border-transparent [&>td]:bg-clip-padding',
                    !isSelected &&
                      'hover:[&>td]:border-transparent hover:[&>td]:bg-surface-subtle hover:[&>td]:shadow-row-gap [tr:hover+&>td]:border-transparent [tr:hover+&>td]:shadow-row-gap',
                    !isSelected &&
                      rowIndex > 0 &&
                      !isRowSelected(rowIndex - 1) &&
                      '[&>td]:border-border-row',
                    !isSelected &&
                      isRowSelected(rowIndex - 1) &&
                      '[&>td]:shadow-row-gap',
                    isSelected &&
                      '[&>td]:bg-surface-fill [&>td]:shadow-row-gap',
                    variant === 'page' &&
                      !isSelected &&
                      'hover:[&>td:first-child]:rounded-l-sm hover:[&>td:last-child]:rounded-r-sm',
                    variant === 'page' &&
                      isSelected &&
                      '[&>td:first-child]:rounded-l-sm [&>td:last-child]:rounded-r-sm'
                  )}
                >
                  {row.getVisibleCells().map((cell) => (
                    <DataTableCell
                      key={cell.id}
                      column={cell.column}
                      variant={variant}
                      size={size}
                    >
                      <table.FlexRender cell={cell} />
                    </DataTableCell>
                  ))}
                </tr>
              );
            })}
      </tbody>
    </table>
  );
}
