import { Icon } from '~/components/icons/icon';
import type { MetricFilter } from '~/utils/metrics';

type MetricFilterChipProps = {
  filter: MetricFilter;
  onToggle: () => void;
  onRemove: () => void;
};

export function MetricFilterChip(props: MetricFilterChipProps) {
  const { filter, onToggle, onRemove } = props;

  return (
    <span className="flex h-7 items-center overflow-hidden rounded-lg border border-border-ring bg-surface-default text-caption shadow-card">
      <span className="px-2 font-mono text-code-small text-text-secondary">{filter.key}</span>
      <button
        type="button"
        onClick={onToggle}
        aria-label={filter.isNegated ? 'Change to equals' : 'Change to not equals'}
        className="h-full px-1.5 text-text-tertiary outline-none hover:bg-surface-fill focus-visible:bg-surface-fill"
      >
        {filter.isNegated ? 'is not' : 'is'}
      </button>
      <span className="truncate px-2 font-medium text-text-primary">{filter.value}</span>
      <button
        type="button"
        onClick={onRemove}
        aria-label={`Remove ${filter.key} filter`}
        className="flex h-full items-center px-1.5 text-icon-secondary outline-none hover:bg-surface-fill focus-visible:bg-surface-fill"
      >
        <Icon name="x" className="size-3" />
      </button>
    </span>
  );
}
