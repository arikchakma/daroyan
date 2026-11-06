import type { Schemas } from '@daroyan/client';
import { useState } from 'react';

import { LogVolumeBar } from '~/components/logs/log-volume-bar';
import { Skeleton } from '~/components/ui/skeleton';
import { Tooltip } from '~/components/ui/tooltip';
import { cn } from '~/utils/classname';
import { pluralize } from '~/utils/format';
import { getVolumeAxisLabel } from '~/utils/logs';

const AXIS_LABELS = 8;
const MIN_SCALE = 3;

type Drag = {
  start: number;
  end: number;
};

type LogVolumeProps = {
  volume?: Schemas['LogVolume'];
  isNow: boolean;
  onZoom: (since: number, until: number) => void;
};

export function LogVolume(props: LogVolumeProps) {
  const { volume, isNow, onZoom } = props;

  const [drag, setDrag] = useState<Drag>();

  const buckets = volume?.buckets ?? [];
  const bucketMs = volume?.bucket_ms ?? 0;
  const top = Math.max(MIN_SCALE, ...buckets.map((bucket) => bucket.info + bucket.warn + bucket.error));
  const errors = buckets.reduce((sum, bucket) => sum + bucket.error, 0);
  const warnings = buckets.reduce((sum, bucket) => sum + bucket.warn, 0);
  const lines = buckets.reduce((sum, bucket) => sum + bucket.info, errors + warnings);
  const axisEvery = Math.max(1, Math.ceil(buckets.length / AXIS_LABELS));
  const low = drag ? Math.min(drag.start, drag.end) : -1;
  const high = drag ? Math.max(drag.start, drag.end) : -1;

  const finishDrag = () => {
    if (!drag || !volume) {
      return;
    }
    setDrag(undefined);
    onZoom(Math.max(buckets[low]!.at, volume.since), Math.min(buckets[high]!.at + bucketMs, volume.until));
  };

  return (
    <section aria-label="Log volume" className="flex flex-col gap-2.5 pt-2.5 pb-1.5">
      <div className="flex items-center justify-between gap-2">
        {volume ? (
          <p className="flex items-baseline gap-1.5 whitespace-nowrap">
            <span className="text-body font-semibold">{pluralize(lines, 'line')}</span>
            <span className="truncate text-caption text-text-tertiary">
              {pluralize(errors, 'error')} · {pluralize(warnings, 'warning')}
            </span>
          </p>
        ) : (
          <Skeleton className="w-48" />
        )}
        <div className="flex items-center gap-3.5 text-caption text-text-tertiary">
          <LegendItem label="Error" className="bg-brand" />
          <LegendItem label="Warning" className="bg-toast-warning-icon" />
          <LegendItem label="Info" className="bg-text-tertiary/50" />
        </div>
      </div>
      <div className="flex flex-col gap-1.5">
        <Tooltip.Provider delay={0} closeDelay={0}>
          <div
            className={cn('group/bars flex h-16 gap-0.5 select-none', !volume && 'animate-pulse')}
            onPointerUp={finishDrag}
            onPointerLeave={() => setDrag(undefined)}
          >
            {buckets.map((bucket, index) => (
              <LogVolumeBar
                key={bucket.at}
                bucket={bucket}
                bucketMs={bucketMs}
                top={top}
                isSelected={index >= low && index <= high}
                onPointerDown={() => setDrag({ start: index, end: index })}
                onPointerEnter={() => drag && setDrag({ ...drag, end: index })}
              />
            ))}
          </div>
        </Tooltip.Provider>
        <div className="flex h-4 justify-between text-caption text-text-tertiary">
          {buckets
            .filter((_, index) => index % axisEvery === 0)
            .map((bucket) => (
              <span key={bucket.at}>{getVolumeAxisLabel(bucket.at, bucketMs)}</span>
            ))}
          {volume && <span>{isNow ? 'Now' : getVolumeAxisLabel(volume.until, bucketMs)}</span>}
        </div>
      </div>
    </section>
  );
}

type LegendItemProps = {
  label: string;
  className: string;
};

function LegendItem(props: LegendItemProps) {
  const { label, className } = props;

  return (
    <span className="flex items-center gap-1.5">
      <span className={cn('size-2 rounded-[2px]', className)} />
      {label}
    </span>
  );
}
