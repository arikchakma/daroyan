import { Checkbox } from '~/components/ui/checkbox';
import type { CellSkeletonShape } from '~/lib/table';
import { cn } from '~/utils/classname';

import { CellBar } from './cell-bar';

const WIDTHS = [62, 48, 74, 40, 56, 68, 44];
const LABEL_WIDTHS = [40, 56, 48, 64, 36, 52];

type CellSkeletonProps = {
  shape: CellSkeletonShape | 'label';
  align?: 'left' | 'right';
  seed: number;
};

export function CellSkeleton(props: CellSkeletonProps) {
  const { shape, align = 'left', seed } = props;

  const width = `${WIDTHS[seed % WIDTHS.length]}%`;
  const secondaryWidth = `${WIDTHS[(seed + 3) % WIDTHS.length]! + 16}%`;

  if (shape === 'none') {
    return null;
  }
  if (shape === 'checkbox') {
    return <Checkbox disabled />;
  }
  if (shape === 'badge') {
    return (
      <span
        aria-hidden
        className={cn(
          'flex h-5.5 items-center',
          align === 'right' && 'justify-end'
        )}
      >
        <span className="h-2.5 w-16 rounded-full bg-surface-pill" />
      </span>
    );
  }
  if (shape === 'label') {
    return (
      <CellBar
        size="caption"
        width={`${LABEL_WIDTHS[seed % LABEL_WIDTHS.length]}px`}
        align={align}
      />
    );
  }
  if (shape === 'lines') {
    return (
      <span aria-hidden className="flex flex-col gap-0.5">
        <CellBar size="caption" width={width} align={align} isPrimary />
        <CellBar size="caption" width={secondaryWidth} align={align} />
      </span>
    );
  }
  if (shape === 'lines-lg') {
    return (
      <span aria-hidden className="flex flex-col py-1.5">
        <CellBar size="body" width={width} align={align} isPrimary />
        <CellBar size="body" width={secondaryWidth} align={align} />
      </span>
    );
  }
  if (shape === 'avatar-text') {
    return (
      <span aria-hidden className="flex items-center gap-2">
        <span className="size-6 shrink-0 rounded-full bg-surface-pill" />
        <span className="flex min-w-0 flex-1">
          <CellBar size="body" width={width} isPrimary />
        </span>
      </span>
    );
  }
  if (
    shape === 'avatar-lines' ||
    shape === 'avatar-lines-lg' ||
    shape === 'icon-lines-lg'
  ) {
    return (
      <span
        aria-hidden
        className={cn(
          'flex items-center',
          shape === 'avatar-lines' && 'gap-2.5',
          shape === 'avatar-lines-lg' && 'gap-2 py-1.5',
          shape === 'icon-lines-lg' && 'gap-2 py-2'
        )}
      >
        <span
          className={cn(
            'shrink-0 bg-surface-pill',
            shape === 'icon-lines-lg'
              ? 'size-4 rounded-sm'
              : 'size-7 rounded-full'
          )}
        />
        <span
          className={cn(
            'flex min-w-0 flex-1 flex-col',
            shape === 'avatar-lines' && 'gap-px'
          )}
        >
          <CellBar
            size={shape === 'avatar-lines' ? 'caption' : 'body'}
            width={width}
            isPrimary
          />
          <CellBar
            size={shape === 'avatar-lines' ? 'caption' : 'body'}
            width={secondaryWidth}
          />
        </span>
      </span>
    );
  }
  return <CellBar size="body" width={width} align={align} isPrimary />;
}
