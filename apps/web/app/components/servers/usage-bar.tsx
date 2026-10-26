import { cn } from '~/utils/classname';
import { formatPercent } from '~/utils/format';

type UsageBarProps = {
  percent?: number | null;
};

export function UsageBar(props: UsageBarProps) {
  const { percent } = props;

  if (percent === undefined || percent === null) {
    return <span className="text-caption font-normal text-text-tertiary">—</span>;
  }

  return (
    <div className="flex items-center gap-2">
      <div className="h-1.5 w-14 overflow-hidden rounded-[3px] bg-surface-row">
        <div
          className="h-full rounded-[3px] bg-chart-primary"
          style={{ width: `${Math.min(percent, 100)}%` }}
        />
      </div>
      <span
        className={cn(
          'text-caption font-normal text-text-primary',
          percent >= 80 && 'text-tone-warning-fg',
          percent >= 90 && 'text-tone-critical-fg'
        )}
      >
        {formatPercent(percent)}
      </span>
    </div>
  );
}
