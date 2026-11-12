import { Icon } from '~/components/icons/icon';
import { cn } from '~/utils/classname';
import type { EditPart } from '~/utils/filter';

type TokenPillProps = {
  name: string;
  condition: string;
  value: string;
  editPart: EditPart | null;
  onEditCondition: () => void;
  onEditValue: () => void;
  onRemove: () => void;
};

export function TokenPill(props: TokenPillProps) {
  const { name, condition, value, editPart, onEditCondition, onEditValue, onRemove } = props;

  const removeButton = (
    <button
      type="button"
      aria-label={`Remove ${name} Filter`}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onRemove}
      className="-mr-0.5 ml-0.5 hidden size-4 items-center justify-center rounded-full outline-none group-hover:flex"
    >
      <Icon name="x" className="size-2.5" />
    </button>
  );

  if (!value) {
    return (
      <span
        className={cn(
          'group flex items-center rounded-sm bg-filter-label pr-1.5 pl-1.5 text-text-primary hover:bg-filter-label-active',
          editPart === 'condition' && 'bg-filter-label-active'
        )}
      >
        <button
          type="button"
          aria-label={`Change ${name} Condition`}
          onMouseDown={(e) => e.preventDefault()}
          onClick={onEditCondition}
          className="outline-none"
        >
          {condition}
        </button>
        {removeButton}
      </span>
    );
  }

  return (
    <>
      <button
        type="button"
        aria-label={`Change ${name} Condition`}
        onMouseDown={(e) => e.preventDefault()}
        onClick={onEditCondition}
        className={cn(
          'flex items-center rounded-l-sm rounded-r-[2px] bg-filter-label pr-1 pl-1.5 text-text-primary outline-none hover:bg-filter-label-active',
          editPart === 'condition' && 'bg-filter-label-active'
        )}
      >
        {condition}
      </button>
      <span
        className={cn(
          'group flex items-center rounded-l-[2px] rounded-r-sm bg-filter-value pr-1.5 pl-1 text-filter-value-fg hover:rounded-r-xs hover:bg-filter-value-hover hover:text-filter-value-fg-hover',
          editPart === 'values' && 'bg-filter-value-active hover:bg-filter-value-active'
        )}
      >
        <button
          type="button"
          aria-label={`${condition} ${value}`}
          onMouseDown={(e) => e.preventDefault()}
          onClick={onEditValue}
          className="outline-none"
        >
          {value}
        </button>
        {removeButton}
      </span>
    </>
  );
}
