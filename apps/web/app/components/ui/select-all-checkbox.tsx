import type { Table } from '@tanstack/react-table';

import { Checkbox } from '~/components/ui/checkbox';
import type { DataTableFeatures } from '~/lib/table';

type SelectAllCheckboxProps<TData extends object> = {
  table: Table<DataTableFeatures, TData>;
  label: string;
};

export function SelectAllCheckbox<TData extends object>(
  props: SelectAllCheckboxProps<TData>
) {
  const { table, label } = props;

  const isAll = table.getIsAllPageRowsSelected();

  return (
    <Checkbox
      aria-label={`Select All ${label} on This Page`}
      checked={isAll}
      indeterminate={table.getIsSomePageRowsSelected() && !isAll}
      onCheckedChange={(checked) => table.toggleAllPageRowsSelected(checked)}
    />
  );
}
