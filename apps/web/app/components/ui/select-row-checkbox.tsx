import type { Row } from '@tanstack/react-table';

import { Checkbox } from '~/components/ui/checkbox';
import type { DataTableFeatures } from '~/lib/table';

type SelectRowCheckboxProps<TData extends object> = {
  row: Row<DataTableFeatures, TData>;
  label: string;
};

export function SelectRowCheckbox<TData extends object>(
  props: SelectRowCheckboxProps<TData>
) {
  const { row, label } = props;

  return (
    <Checkbox
      aria-label={`Select ${label}`}
      checked={row.getIsSelected()}
      onCheckedChange={(checked) => row.toggleSelected(checked)}
      className="relative z-1"
    />
  );
}
