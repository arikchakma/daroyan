import type { Schemas } from '@daroyan/client';
import { barY, defineChart, stack } from '@tanstack/charts';
import { controlledSignal } from '@tanstack/charts/interaction/signal';
import { Chart } from '@tanstack/charts/react/tooltip';
import { scaleBand } from '@tanstack/charts/scales/band';
import { scaleLinear } from '@tanstack/charts/scales/linear';
import { keyedSelection } from '@tanstack/charts/selection';
import { tooltip } from '@tanstack/charts/tooltip';
import { portal } from '@tanstack/charts/tooltip/portal';
import { useMemo } from 'react';

import { Skeleton } from '~/components/ui/skeleton';
import { cn } from '~/utils/classname';
import { formatCount, pluralize } from '~/utils/format';
import { getBucketRange, getVolumeAxisLabel } from '~/utils/logs';

const CHART_HEIGHT = 84;
const AXIS_LABELS = 8;
const MIN_SCALE = 3;
const LEVELS = ['error', 'warn', 'info'] as const;
const LEVEL_FILLS = {
  error: 'var(--color-brand)',
  warn: 'var(--color-toast-warning-icon)',
  info: 'color-mix(in srgb, var(--color-text-tertiary) 50%, transparent)',
};

type Level = (typeof LEVELS)[number];

type VolumeRow = {
  at: number;
  level: Level;
  count: number;
};

type LogVolumeProps = {
  volume?: Schemas['LogVolume'];
  onZoom: (since: number, until: number) => void;
};

export function LogVolume(props: LogVolumeProps) {
  const { volume, onZoom } = props;

  const buckets = volume?.buckets ?? [];
  const errors = buckets.reduce((sum, bucket) => sum + bucket.error, 0);
  const warnings = buckets.reduce((sum, bucket) => sum + bucket.warn, 0);
  const lines = buckets.reduce((sum, bucket) => sum + bucket.info, errors + warnings);

  const { definition, bucketsByTime } = useMemo(() => {
    const buckets = volume?.buckets ?? [];
    const bucketMs = volume?.bucket_ms ?? 0;
    const times = buckets.map((bucket) => bucket.at);
    const bucketsByTime = new Map(buckets.map((bucket) => [bucket.at, bucket]));
    const rows: VolumeRow[] = buckets.flatMap((bucket) =>
      LEVELS.map((level) => ({ at: bucket.at, level, count: bucket[level] }))
    );
    const top = Math.max(MIN_SCALE, ...buckets.map((bucket) => bucket.info + bucket.warn + bucket.error));
    const axisEvery = Math.max(1, Math.ceil(times.length / AXIS_LABELS));
    const selection = keyedSelection<VolumeRow, number, number, number>({
      selected: controlledSignal<number | null>(null, (at) => {
        if (volume && at !== null) {
          onZoom(Math.max(at, volume.since), Math.min(at + bucketMs, volume.until));
        }
      }),
      key: (row) => row.at,
    });

    const definition = defineChart({
      marks: [
        barY(rows, {
          x: 'at',
          y: 'count',
          z: 'level',
          fill: (row) => LEVEL_FILLS[row.level],
          layout: stack({ order: [...LEVELS] }),
          radius: { end: 2 },
        }),
      ],
      scales: {
        x: {
          scale: () => scaleBand<number>().domain(times).paddingInner(0.2),
          axis: {
            line: false,
            ticks: {
              values: times.filter((_, index) => index % axisEvery === 0),
              size: 0,
              format: (at: number) => getVolumeAxisLabel(at, bucketMs),
            },
            tickLabels: { fontSize: 11, thin: false },
          },
        },
        y: {
          scale: scaleLinear().domain([0, top]),
          axis: false,
        },
      },
      theme: { foreground: 'var(--color-text-tertiary)' },
      focus: 'group-x',
      tooltip: { use: tooltip, portal, sticky: false, anchor: 'pointer', placement: ['top', 'right', 'left', 'bottom'] },
      selection,
    });

    return { definition, bucketsByTime };
  }, [volume, onZoom]);

  return (
    <section aria-label="Log volume" className="flex flex-col gap-2 pt-2.5">
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
      <Chart
        definition={definition}
        height={CHART_HEIGHT}
        initialWidth={960}
        ariaLabel="Log lines over time by level"
        className={cn('ts-chart-host text-axis [&_svg]:outline-none', !volume && 'animate-pulse')}
        renderTooltipBody={({ points }) => {
          const at = points[0]?.datum.at;
          const bucket = at === undefined ? undefined : bucketsByTime.get(at);
          if (!bucket || !volume) {
            return null;
          }

          return (
            <div className="flex min-w-36 flex-col gap-1 rounded-lg border border-border-default bg-surface-default p-2.5 shadow-popover">
              <span className="text-caption font-medium text-text-primary">
                {getBucketRange(bucket.at, volume.bucket_ms)}
              </span>
              <TooltipRow label="Lines" value={bucket.info + bucket.warn + bucket.error} />
              <TooltipRow label="Errors" value={bucket.error} className="bg-brand" />
              <TooltipRow label="Warnings" value={bucket.warn} className="bg-toast-warning-icon" />
            </div>
          );
        }}
      />
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

type TooltipRowProps = {
  label: string;
  value: number;
  className?: string;
};

function TooltipRow(props: TooltipRowProps) {
  const { label, value, className } = props;

  return (
    <div className="flex items-center gap-2 text-caption">
      <span className={cn('size-1.75 rounded-[2px] bg-text-tertiary/50', className)} />
      <span className="flex-1 text-text-secondary">{label}</span>
      <span className="pl-4 font-medium text-text-primary">{formatCount(value)}</span>
    </div>
  );
}
