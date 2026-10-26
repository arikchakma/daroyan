import { Icon } from '~/components/icons/icon';
import { cn } from '~/utils/classname';

type FilterOptionProps = {
  id: string;
  label: string;
  indicator: 'none' | 'checkbox' | 'check';
  isActive: boolean;
  isChecked: boolean;
  isSelected: boolean;
  onHover: () => void;
  onSelect: () => void;
};

export function FilterOption(props: FilterOptionProps) {
  const {
    id,
    label,
    indicator,
    isActive,
    isChecked,
    isSelected,
    onHover,
    onSelect,
  } = props;

  return (
    <div
      id={id}
      role="option"
      aria-label={label}
      aria-selected={isSelected}
      onMouseMove={onHover}
      onClick={onSelect}
      className={cn(
        'flex h-7 shrink-0 cursor-default items-center gap-2 rounded-sm px-2 text-body',
        isActive && 'bg-surface-subtle'
      )}
    >
      {indicator === 'checkbox' && (
        <span
          aria-hidden="true"
          className={cn(
            'flex size-4 shrink-0 items-center justify-center rounded-xs border border-border-strong bg-surface-default text-text-on-primary shadow-button-secondary',
            isChecked && 'border-transparent bg-button-primary shadow-none'
          )}
        >
          {isChecked && <Icon name="check" className="size-3" />}
        </span>
      )}
      {indicator === 'check' && (
        <span className="flex size-5 shrink-0 items-center justify-center">
          {isChecked && <Icon name="check" />}
        </span>
      )}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {isActive && (
        <span
          aria-hidden="true"
          className="flex h-5 shrink-0 items-center gap-0.5 rounded-sm border border-filter-label-active bg-filter-label px-1 text-caption"
        >
          <Icon name="corner-down-left" className="size-3" />
          Enter
        </span>
      )}
    </div>
  );
}
