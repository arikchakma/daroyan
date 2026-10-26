import type { Ref } from 'react';

import { DropdownStatus } from '~/components/ui/dropdown-status';
import { cn } from '~/utils/classname';
import { getConditionLabel } from '~/utils/filter';
import type {
  EditPart,
  FilterItem,
  SearchFilterField,
  SearchFilterValue,
} from '~/utils/filter';

import { FilterOption } from './filter-option';

type FilterListProps = {
  ref: Ref<HTMLDivElement>;
  id: string;
  items: FilterItem[];
  activeKey: string | null;
  field: SearchFilterField | null;
  part: EditPart;
  value: SearchFilterValue;
  text: string;
  isEmpty: boolean;
  onHover: (key: string) => void;
  onSelect: (item: FilterItem) => void;
  onRetry: () => void;
  onLeave: (target: Element | null) => void;
};

export function FilterList(props: FilterListProps) {
  const {
    ref,
    id,
    items,
    activeKey,
    field,
    part,
    value,
    text,
    isEmpty,
    onHover,
    onSelect,
    onRetry,
    onLeave,
  } = props;

  return (
    <div
      ref={ref}
      id={id}
      role="listbox"
      aria-label={field ? `${field.label} Values` : 'Filters'}
      aria-multiselectable={(field?.multiple && part === 'values') || undefined}
      onMouseDown={(e) => e.preventDefault()}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget)) {
          onLeave(e.relatedTarget);
        }
      }}
      className={cn(
        'absolute top-full z-50 mt-2 flex w-44 flex-col rounded-lg bg-surface-default p-1 shadow-dropdown ring-1 ring-border-elevated ring-inset',
        field && part === 'values' && 'w-52',
        field && part === 'condition' && 'w-40'
      )}
    >
      {field && part === 'values' && (
        <DropdownStatus
          plural={field.plural ?? 'options'}
          search={text.trim()}
          variant="list"
          isLoading={!!field.isLoading}
          isFailed={!!field.isFailed}
          isEmpty={isEmpty}
          onRetry={onRetry}
        />
      )}
      {items.map((item, index) => {
        const isActive = item.key === activeKey;
        const isChecked =
          (item.kind === 'value' && value.values.includes(item.value)) ||
          (item.kind === 'operator' && value.negate === item.negate);
        const label =
          item.kind !== 'field'
            ? item.label
            : item.negate
              ? getConditionLabel({ field: item.field, negate: true })
              : item.field.label;
        const isFirstOperator =
          item.kind === 'operator' &&
          (items[index - 1]?.kind === 'value' || (index === 0 && isEmpty));

        return (
          <div key={item.key} className="flex flex-col">
            {isFirstOperator && (
              <div className="-mx-1 my-1 h-px shrink-0 bg-border-row" />
            )}
            <FilterOption
              id={`${id}-${index}`}
              label={label}
              indicator={
                item.kind === 'field'
                  ? 'none'
                  : item.kind === 'value' && field?.multiple
                    ? 'checkbox'
                    : 'check'
              }
              isActive={isActive}
              isChecked={isChecked}
              isSelected={item.kind === 'field' ? isActive : isChecked}
              onHover={() => onHover(item.key)}
              onSelect={() => onSelect(item)}
            />
          </div>
        );
      })}
    </div>
  );
}
