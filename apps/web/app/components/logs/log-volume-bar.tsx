import type { Schemas } from '@daroyan/client';

import { Tooltip } from '~/components/ui/tooltip';
import { cn } from '~/utils/classname';
import { pluralize } from '~/utils/format';
import { getBucketRange } from '~/utils/logs';

type LogVolumeBarProps = {
  bucket: Schemas['LogBucket'];
  bucketMs: number;
  top: number;
  isSelected: boolean;
  onPointerDown: () => void;
  onPointerEnter: () => void;
};

export function LogVolumeBar(props: LogVolumeBarProps) {
  const { bucket, bucketMs, top, isSelected, onPointerDown, onPointerEnter } = props;

  const total = bucket.info + bucket.warn + bucket.error;
  const range = getBucketRange(bucket.at, bucketMs);

  return (
    <Tooltip.Root>
      <Tooltip.Trigger
        render={<span />}
        aria-label={`${range}: ${pluralize(total, 'line')}, ${pluralize(bucket.error, 'error')}, ${pluralize(bucket.warn, 'warning')}`}
        onPointerDown={(event) => {
          event.preventDefault();
          onPointerDown();
        }}
        onPointerEnter={onPointerEnter}
        className={cn(
          'flex min-w-0.5 flex-1 cursor-pointer flex-col justify-end gap-px rounded-xs transition-opacity group-hover/bars:opacity-50 hover:opacity-100!',
          isSelected && 'bg-surface-fill opacity-100!'
        )}
      >
        {total === 0 ? (
          <span className="h-0.75 rounded-[1px] bg-surface-fill" />
        ) : (
          <>
            {bucket.info > 0 && (
              <span className="min-h-0.5 rounded-[2px] bg-text-tertiary/50" style={{ height: `${(bucket.info / top) * 100}%` }} />
            )}
            {bucket.warn > 0 && (
              <span
                className="min-h-0.5 rounded-[2px] bg-toast-warning-icon"
                style={{ height: `${(bucket.warn / top) * 100}%` }}
              />
            )}
            {bucket.error > 0 && (
              <span className="min-h-0.5 rounded-[2px] bg-brand" style={{ height: `${(bucket.error / top) * 100}%` }} />
            )}
          </>
        )}
      </Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Positioner side="top">
          <Tooltip.Popup className="flex-col items-start gap-0.5">
            <span>{range}</span>
            <span className="font-normal opacity-80">{pluralize(total, 'line')}</span>
            <span className="font-normal opacity-80">{pluralize(bucket.error, 'error')}</span>
            <span className="font-normal opacity-80">{pluralize(bucket.warn, 'warning')}</span>
            <Tooltip.Arrow />
          </Tooltip.Popup>
        </Tooltip.Positioner>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}
