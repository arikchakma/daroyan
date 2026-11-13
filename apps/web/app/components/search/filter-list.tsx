import type { Ref } from 'react';

import { DropdownNote } from '~/components/ui/dropdown-note';
import { DropdownStatus } from '~/components/ui/dropdown-status';
import { cn } from '~/utils/classname';
import {
  getItemIndicator,
  getItemLabel,
  hasSeparatorBefore,
  isItemChecked,
} from '~/utils/filter';
import type {
  EditPart,
  FilterItem,
  SearchFilterField,
  SearchFilterValue,
} from '~/utils/filter';
import type { SearchOperator } from '~/utils/log-search';

import { FilterError } from './filter-error';
import { FilterOption } from './filter-option';
import { FilterTermsLabel } from './filter-terms-label';

type FilterListProps = {
  ref: Ref<HTMLDivElement>;
  id: string;
  items: FilterItem[];
  activeKey: string | null;
  field: SearchFilterField | null;
  part: EditPart;
  value: SearchFilterValue;
  operator: SearchOperator | null;
  error: string | null;
  hint: string | null;
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
    operator,
    error,
    hint,
    text,
    isEmpty,
    onHover,
    onSelect,
    onRetry,
    onLeave,
  } = props;

  const hasText = !!error || items.some((item) => item.kind === 'terms');

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
        field && part === 'condition' && 'w-40',
        hasText && 'w-max max-w-3xl min-w-52'
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
      {error && <FilterError message={error} />}
      {hint && <DropdownNote>{hint}</DropdownNote>}
      {items.map((item, index) => {
        const isActive = item.key === activeKey;
        const isChecked = isItemChecked({ item, value, operator });

        return (
          <div key={item.key} className="flex flex-col">
            {hasSeparatorBefore(items, index, isEmpty) && (
              <div className="-mx-1 my-1 h-px shrink-0 bg-border-row" />
            )}
            <FilterOption
              id={`${id}-${index}`}
              label={getItemLabel(item)}
              indicator={getItemIndicator(item, field)}
              isActive={isActive}
              isChecked={isChecked}
              isSelected={item.kind === 'field' || item.kind === 'terms' ? isActive : isChecked}
              onHover={() => onHover(item.key)}
              onSelect={() => onSelect(item)}
            >
              {item.kind === 'terms' ? <FilterTermsLabel terms={item.terms} /> : null}
            </FilterOption>
          </div>
        );
      })}
    </div>
  );
}
