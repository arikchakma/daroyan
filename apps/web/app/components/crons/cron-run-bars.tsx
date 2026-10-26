import type { Schemas } from '@daroyan/client';

import { Tooltip } from '~/components/ui/tooltip';
import { cn } from '~/utils/classname';
import { formatClock, formatDate } from '~/utils/format';

type CronRunBarsProps = {
  runs: Schemas['CronRunBar'][];
  count: number;
  barClassName?: string;
  className?: string;
};

const LABELS: Record<string, string> = {
  ok: 'OK',
  late: 'Late',
  failed: 'Failed',
  missed: 'Missed',
};

function getColor(status: string) {
  switch (status) {
    case 'ok':
      return 'bg-tone-success-solid';
    case 'late':
      return 'bg-toast-warning-icon';
    default:
      return 'bg-brand';
  }
}

export function CronRunBars(props: CronRunBarsProps) {
  const { runs, count, barClassName = 'min-w-0.5 flex-1', className } = props;

  const empty = Array.from({ length: Math.max(count - runs.length, 0) }, (_, index) => index);

  return (
    <Tooltip.Provider delay={0} closeDelay={0}>
      <div className={cn('relative z-1 flex h-5 items-stretch gap-0.5', className)}>
        {empty.map((index) => (
          <span key={index} className={cn('rounded-[1px] bg-surface-fill', barClassName)} />
        ))}
        {runs.map((run, index) => (
          <Tooltip.Root key={`${run.at}-${index}`}>
            <Tooltip.Trigger
              render={<span />}
              aria-label={`${formatDate(run.at)}, ${formatClock(run.at)}: ${LABELS[run.status] ?? run.status}`}
              className={cn('rounded-[1px] hover:opacity-70', barClassName, getColor(run.status))}
            />
            <Tooltip.Portal>
              <Tooltip.Positioner side="top">
                <Tooltip.Popup className="flex-col items-start gap-0.5">
                  <span>
                    {formatDate(run.at)}, {formatClock(run.at)}
                  </span>
                  <span className="font-normal opacity-80">{LABELS[run.status] ?? run.status}</span>
                  <Tooltip.Arrow />
                </Tooltip.Popup>
              </Tooltip.Positioner>
            </Tooltip.Portal>
          </Tooltip.Root>
        ))}
      </div>
    </Tooltip.Provider>
  );
}
