import { Fragment } from 'react';

import { Select } from '~/components/ui/select';
import type { SelectItemData } from '~/utils/rule-form';

type PlainSelectProps = {
  labelId: string;
  value: string;
  onValueChange: (value: string | null) => void;
  items?: SelectItemData[];
  groups?: SelectItemData[][];
};

export function PlainSelect(props: PlainSelectProps) {
  const { labelId, value, onValueChange, items, groups } = props;

  const sets = groups ?? [items ?? []];

  return (
    <Select.Root items={sets.flat()} value={value} onValueChange={(next) => onValueChange(next as string | null)}>
      <Select.Trigger aria-labelledby={labelId}>
        <Select.Value />
      </Select.Trigger>
      <Select.Portal>
        <Select.Positioner>
          <Select.Popup>
            {sets.map((set, index) => (
              <Fragment key={index}>
                {index > 0 && <Select.Separator />}
                {set.map((item) => (
                  <Select.Item key={item.value} value={item.value}>
                    {item.label}
                  </Select.Item>
                ))}
              </Fragment>
            ))}
          </Select.Popup>
        </Select.Positioner>
      </Select.Portal>
    </Select.Root>
  );
}
