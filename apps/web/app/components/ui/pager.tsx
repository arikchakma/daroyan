import { Icon } from '~/components/icons/icon';
import { cn } from '~/utils/classname';

type PagerProps = {
  start: number;
  rowCount: number;
  hasPrevious: boolean;
  hasNext: boolean;
  onPrevious: () => void;
  onNext: () => void;
  className?: string;
};

export function Pager(props: PagerProps) {
  const { start, rowCount, hasPrevious, hasNext, onPrevious, onNext, className } = props;

  if (!hasPrevious && !hasNext) {
    return null;
  }

  return (
    <nav
      aria-label="Pagination"
      className={cn(
        'sticky bottom-3 z-10 mt-3 flex w-fit items-center gap-1 self-start rounded-full border border-border-ring bg-surface-default p-1 shadow-float',
        className
      )}
    >
      <button
        type="button"
        aria-label="Previous Page"
        disabled={!hasPrevious}
        onClick={onPrevious}
        className="flex size-7 items-center justify-center rounded-full text-text-primary outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 disabled:pointer-events-none disabled:text-border-strong"
      >
        <Icon name="chevron-left" />
      </button>
      <span className="text-caption text-text-tertiary tabular-nums">
        {start}–{start + Math.max(rowCount, 1) - 1}
      </span>
      <button
        type="button"
        aria-label="Next Page"
        disabled={!hasNext}
        onClick={onNext}
        className="flex size-7 items-center justify-center rounded-full text-text-primary outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 disabled:pointer-events-none disabled:text-border-strong"
      >
        <Icon name="chevron-right" />
      </button>
    </nav>
  );
}
