import { Fragment, useId } from 'react';

import { Select } from '~/components/ui/select';
import { cn } from '~/utils/classname';
import type { SelectItemData } from '~/utils/rule-form';

type SelectFieldProps = {
  label: string;
  hint?: string;
  placeholder?: string;
  value: string | null;
  onValueChange: (value: string | null) => void;
  items?: SelectItemData[];
  groups?: { label: string; items: SelectItemData[] }[];
  separatorBefore?: string;
  className?: string;
};

export function SelectField(props: SelectFieldProps) {
  const { label, hint, placeholder, value, onValueChange, items, groups, separatorBefore, className } = props;

  const labelId = useId();
  const hintId = useId();

  const allItems = groups ? groups.flatMap((group) => group.items) : (items ?? []);

  return (
    <div className={cn('flex min-w-0 flex-col gap-1.5', className)}>
      <span id={labelId} className="text-body font-medium">
        {label}
      </span>
      <Select.Root items={allItems} value={value} onValueChange={(next) => onValueChange(next as string | null)}>
        <Select.Trigger aria-labelledby={labelId} aria-describedby={hint ? hintId : undefined}>
          <Select.Value placeholder={placeholder} />
        </Select.Trigger>
        <Select.Portal>
          <Select.Positioner>
            <Select.Popup>
              {groups
                ? groups.map((group, index) => (
                    <Fragment key={group.label}>
                      {index > 0 && <Select.Separator />}
                      <Select.Group>
                        <Select.GroupLabel>{group.label}</Select.GroupLabel>
                        {group.items.map((item) => (
                          <Select.Item key={item.value} value={item.value}>
                            {item.label}
                          </Select.Item>
                        ))}
                      </Select.Group>
                    </Fragment>
                  ))
                : allItems.map((item) => (
                    <Fragment key={item.value}>
                      {item.value === separatorBefore && <Select.Separator />}
                      <Select.Item value={item.value}>{item.label}</Select.Item>
                    </Fragment>
                  ))}
            </Select.Popup>
          </Select.Positioner>
        </Select.Portal>
      </Select.Root>
      {hint && (
        <p id={hintId} className="text-caption text-text-secondary">
          {hint}
        </p>
      )}
    </div>
  );
}
