import type { Schemas } from '@daroyan/client';
import { cva } from 'class-variance-authority';

import { Tooltip } from '~/components/ui/tooltip';
import { cn } from '~/utils/classname';
import { HOUR_MS, formatClock } from '~/utils/format';
import { getBarTone } from '~/utils/overview';

const TREND_HOURS = 24;

const trendBar = cva('absolute inset-x-0 bottom-0', {
  variants: {
    tone: {
      normal: '',
      warning: 'bg-toast-warning-icon',
      critical: 'bg-brand',
    },
  },
});

type TrendBarsProps = {
  points: Schemas['TrendPoint'][];
  label: (value: number) => string;
  minScale: number;
  max?: number;
  fillClassName: string;
  warnAt?: number;
  criticalAt?: number;
};

export function TrendBars(props: TrendBarsProps) {
  const { points, label, minScale, max, fillClassName, warnAt, criticalAt } = props;

  const byHour = new Map(points.map((point) => [point.at, point.value]));
  const lastHour = Math.floor(Date.now() / HOUR_MS) * HOUR_MS;
  const hours = Array.from({ length: TREND_HOURS }, (_, index) => lastHour - (TREND_HOURS - 1 - index) * HOUR_MS);
  const top = max ?? Math.max(minScale, ...points.map((point) => point.value)) * 1.25;

  return (
    <Tooltip.Provider delay={0} closeDelay={0}>
      <div className="relative">
        <div className="flex h-7 items-stretch gap-0.5">
          {hours.map((hour) => {
            const value = byHour.get(hour);
            const tone = value === undefined ? 'normal' : getBarTone({ value, warnAt, criticalAt });
            const text = value === undefined ? 'No data' : label(value);

            return (
              <Tooltip.Root key={hour}>
                <Tooltip.Trigger
                  render={<span />}
                  aria-label={`${formatClock(hour)}: ${text}`}
                  className="relative min-w-0.5 flex-1 overflow-hidden rounded-[1px] bg-surface-fill hover:opacity-70"
                >
                  {value !== undefined && value > 0 && (
                    <span
                      className={cn(trendBar({ tone }), tone === 'normal' && fillClassName)}
                      style={{ height: `${Math.max((value / top) * 100, 4)}%` }}
                    />
                  )}
                </Tooltip.Trigger>
                <Tooltip.Portal>
                  <Tooltip.Positioner side="top">
                    <Tooltip.Popup className="flex-col items-start gap-0.5">
                      <span>{formatClock(hour)}</span>
                      <span className="font-normal opacity-80">{text}</span>
                      <Tooltip.Arrow />
                    </Tooltip.Popup>
                  </Tooltip.Positioner>
                </Tooltip.Portal>
              </Tooltip.Root>
            );
          })}
        </div>
        {max !== undefined && warnAt !== undefined && (
          <span
            aria-hidden
            className="pointer-events-none absolute inset-x-0 border-t border-dashed border-toast-warning-icon"
            style={{ bottom: `${(warnAt / max) * 100}%` }}
          />
        )}
      </div>
    </Tooltip.Provider>
  );
}
