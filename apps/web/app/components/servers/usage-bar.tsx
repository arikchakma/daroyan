import { cn } from '~/utils/classname';
import { formatPercent } from '~/utils/format';
import { getUsageTone } from '~/utils/servers';

type UsageBarProps = {
  percent?: number | null;
};

export function UsageBar(props: UsageBarProps) {
  const { percent } = props;

  if (percent === undefined || percent === null) {
    return <span className="text-caption font-normal text-text-tertiary">—</span>;
  }

  const tone = getUsageTone(percent);

  return (
    <div className="flex items-center gap-2">
      <div className="h-1.5 w-14 overflow-hidden rounded-[3px] bg-surface-fill-strong">
        <div
          className="h-full rounded-[3px] bg-chart-primary"
          style={{ width: `${Math.min(percent, 100)}%` }}
        />
      </div>
      <span
        className={cn(
          'text-caption font-normal text-text-primary',
          tone === 'warn' && 'text-tone-warning-fg',
          tone === 'crit' && 'text-tone-critical-fg'
        )}
      >
        {formatPercent(percent)}
      </span>
    </div>
  );
}
