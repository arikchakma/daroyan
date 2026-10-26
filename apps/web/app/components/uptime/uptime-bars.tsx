import type { Schemas } from '@daroyan/client';

import { Tooltip } from '~/components/ui/tooltip';
import { cn } from '~/utils/classname';
import { formatDate } from '~/utils/format';
import { formatUptime } from '~/utils/uptime';

type UptimeBarsProps = {
  days: Schemas['UptimeDay'][];
  className?: string;
};

function describeDay(day: Schemas['UptimeDay']) {
  if (day.total === 0) {
    return 'No checks';
  }
  const parts = [`${formatUptime((day.total - day.down) / day.total)} uptime`];
  if (day.down > 0) {
    parts.push(`${day.down} failed`);
  }
  if (day.slow > 0) {
    parts.push(`${day.slow} slow`);
  }
  return parts.join(' · ');
}

export function UptimeBars(props: UptimeBarsProps) {
  const { days, className } = props;

  return (
    <Tooltip.Provider delay={0} closeDelay={0}>
      <div className={cn('relative z-1 flex h-5 items-stretch gap-0.5', className)}>
        {days.map((day) => (
          <Tooltip.Root key={day.day}>
            <Tooltip.Trigger
              render={<span />}
              aria-label={`${formatDate(day.day)}: ${describeDay(day)}`}
              className={cn(
                'min-w-0.5 flex-1 rounded-[1px] hover:opacity-70',
                day.total === 0
                  ? 'bg-surface-fill'
                  : day.down > 0
                    ? 'bg-brand'
                    : day.slow > 0
                      ? 'bg-toast-warning-icon'
                      : 'bg-tone-success-solid'
              )}
            />
            <Tooltip.Portal>
              <Tooltip.Positioner side="top">
                <Tooltip.Popup className="flex-col items-start gap-0.5">
                  <span>{formatDate(day.day)}</span>
                  <span className="font-normal opacity-80">{describeDay(day)}</span>
                  {day.total > 0 && (
                    <span className="font-normal opacity-80">
                      {day.total.toLocaleString('en-US')} {day.total === 1 ? 'check' : 'checks'}
                    </span>
                  )}
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
