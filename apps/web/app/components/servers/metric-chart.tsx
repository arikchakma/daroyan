import { areaY, defineChart, lineY, ruleY, text } from '@tanstack/charts';
import { crosshair } from '@tanstack/charts/crosshair';
import { Chart } from '@tanstack/charts/react/tooltip';
import { scaleLinear } from '@tanstack/charts/scales/linear';
import { tooltip } from '@tanstack/charts/tooltip';
import { useMemo } from 'react';
import type { ReactNode } from 'react';

import { SMOOTH_CURVE } from '~/utils/chart';
import { cn } from '~/utils/classname';
import { DAY_MS, formatClock, formatDate } from '~/utils/format';

const CHART_HEIGHT = 112;

export type ChartSeries = {
  label: string;
  values: (number | null)[];
  isSecondary?: boolean;
  color?: string;
};

type ChartRow = {
  at: number;
  series: string;
  value: number | null;
};

type MetricChartProps = {
  title: string;
  value: string;
  detail?: string;
  isValueCritical?: boolean;
  series: ChartSeries[];
  times: number[];
  from: number;
  to: number;
  bucketMs: number;
  max: number;
  maxLabel: string;
  formatValue: (value: number) => string;
  limit?: number;
  limitLabel?: string;
  legend?: ReactNode;
  footer?: ReactNode;
  height?: number;
};

function formatTick(value: number, from: number, to: number) {
  if (value === to) {
    return 'Now';
  }
  return to - from > DAY_MS ? formatDate(value) : formatClock(value);
}

function formatTooltipTime(value: number, from: number, to: number) {
  return to - from > DAY_MS ? `${formatDate(value)}, ${formatClock(value)}` : formatClock(value);
}

function getLineColor(line: Pick<ChartSeries, 'isSecondary' | 'color'>) {
  if (line.color) {
    return line.color;
  }
  return line.isSecondary ? 'var(--color-chart-secondary)' : 'var(--color-chart-primary)';
}

function buildRows(series: ChartSeries[], times: number[], bucketMs: number) {
  const rows: ChartRow[] = [];
  times.forEach((at, index) => {
    const previous = times[index - 1];
    if (previous !== undefined && at - previous > bucketMs * 1.5) {
      for (const line of series) {
        rows.push({ at: previous + bucketMs, series: line.label, value: null });
      }
    }
    for (const line of series) {
      rows.push({ at, series: line.label, value: line.values[index] ?? null });
    }
  });
  return rows;
}

export function MetricChart(props: MetricChartProps) {
  const {
    title,
    value,
    detail,
    isValueCritical,
    series,
    times,
    from,
    to,
    bucketMs,
    max,
    maxLabel,
    formatValue,
    limit,
    limitLabel,
    legend,
    footer,
    height = CHART_HEIGHT,
  } = props;

  const { definition, rowsByTime } = useMemo(() => {
    const rows = buildRows(series, times, bucketMs);
    const rowsByTime = new Map<number, ChartRow[]>();
    for (const row of rows) {
      if (row.value !== null) {
        rowsByTime.set(row.at, [...(rowsByTime.get(row.at) ?? []), row]);
      }
    }

    const hasLimit = limit !== undefined && limit <= max;
    const lines = series.slice().reverse();

    const definition = defineChart({
      gradients: series.map((line, index) => ({
        id: `area-${index}`,
        x1: 0,
        y1: 0,
        x2: 0,
        y2: 1,
        stops: [
          { offset: 0, color: getLineColor(line), opacity: 0.16 },
          { offset: 1, color: getLineColor(line), opacity: 0 },
        ],
      })),
      marks: [
        crosshair({ x: { stroke: 'var(--color-border-strong)' }, y: false }),
        ...lines.map((line) =>
          areaY(
            rows.filter((row) => row.series === line.label),
            {
              id: `${line.label}-area`,
              x: 'at',
              y: 'value',
              fill: `url(#area-${series.indexOf(line)})`,
              curve: SMOOTH_CURVE,
            }
          )
        ),
        ...(hasLimit
          ? [
              ruleY([limit], {
                stroke: 'var(--color-tone-critical-fg)',
                strokeOpacity: 0.8,
                strokeDasharray: '3 3',
              }),
            ]
          : []),
        ...(hasLimit && limitLabel
          ? [
              text([{ at: from, value: limit }], {
                x: 'at',
                y: 'value',
                text: () => limitLabel,
                fill: 'var(--color-tone-critical-fg)',
                fontSize: 11,
                anchor: 'start',
                dy: -6,
              }),
            ]
          : []),
        ...lines.map((line) =>
          lineY(
            rows.filter((row) => row.series === line.label),
            {
              id: line.label,
              x: 'at',
              y: 'value',
              stroke: getLineColor(line),
              strokeWidth: 2,
              curve: SMOOTH_CURVE,
            }
          )
        ),
      ],
      scales: {
        x: {
          scale: scaleLinear().domain([from, to]),
          axis: {
            line: false,
            ticks: {
              values: [from, from + (to - from) / 2, to],
              size: 0,
              format: (tick: number) => formatTick(tick, from, to),
            },
            tickLabels: {
              fontSize: 11,
              thin: false,
              anchor: ({ value: tick }) => (tick === from ? 'start' : tick === to ? 'end' : 'middle'),
            },
          },
        },
        y: {
          scale: scaleLinear().domain([0, max]),
          grid: { stroke: 'var(--color-chart-grid)', strokeOpacity: 1 },
          axis: {
            line: false,
            ticks: { values: [0, max / 2, max], size: 0 },
            tickLabels: false,
          },
        },
      },
      theme: { foreground: 'var(--color-text-tertiary)' },
      focus: 'group-x',
      maxFocusDistance: Number.POSITIVE_INFINITY,
      tooltip: {
        use: tooltip,
        anchor: 'pointer',
        placement: ['right', 'left'],
      },
    });

    return { definition, rowsByTime };
  }, [series, times, from, to, bucketMs, max, limit, limitLabel]);

  return (
    <div className="@container flex min-w-0 flex-col gap-2.5 rounded-2xl border border-border-ring bg-surface-default px-4 pt-3.5 pb-3 shadow-card">
      <div className="flex flex-col gap-1.5 @[13rem]:flex-row @[13rem]:items-start">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5 whitespace-nowrap">
          <span className="text-caption font-medium text-text-secondary">{title}</span>
          <div className="flex items-baseline gap-1.5">
            <span
              className={cn(
                'text-body font-semibold text-text-primary',
                isValueCritical && 'text-tone-critical-fg'
              )}
            >
              {value}
            </span>
            {detail && <span className="truncate text-caption font-normal text-text-tertiary">{detail}</span>}
          </div>
        </div>
        {legend}
      </div>
      <div className="relative">
        <span className="pointer-events-none absolute top-0.5 left-0 z-10 bg-surface-default pr-1 text-axis text-text-tertiary">
          {maxLabel}
        </span>
        <Chart
          definition={definition}
          height={height}
          initialWidth={240}
          ariaLabel={`${title} over time`}
          className="ts-chart-host text-axis [&_svg]:outline-none"
          renderTooltipBody={({ points }) => {
            const at = points[0]?.datum.at;
            const rows = at === undefined ? [] : (rowsByTime.get(at) ?? []);
            return (
              <div className="flex min-w-36 flex-col gap-1 rounded-lg border border-border-default bg-surface-default p-2.5 shadow-popover">
                <span className="text-caption font-medium text-text-primary">
                  {at === undefined ? '' : formatTooltipTime(at, from, to)}
                </span>
                {rows.map((row) => {
                  const line = series.find((item) => item.label === row.series);
                  return (
                    <div key={row.series} className="flex items-center gap-2 text-caption">
                      <span
                        className="size-1.75 rounded-full"
                        style={{ backgroundColor: line ? getLineColor(line) : undefined }}
                      />
                      <span className="flex-1 text-text-secondary">{row.series}</span>
                      <span className="pl-4 font-medium text-text-primary">{formatValue(row.value ?? 0)}</span>
                    </div>
                  );
                })}
              </div>
            );
          }}
        />
      </div>
      {footer}
    </div>
  );
}

type ChartLegendProps = {
  items: { label: string; isSecondary?: boolean; color?: string }[];
};

export function ChartLegend(props: ChartLegendProps) {
  const { items } = props;

  return (
    <div className="flex shrink-0 flex-wrap gap-x-3 gap-y-0.5 @[13rem]:flex-col @[13rem]:items-end">
      {items.map((item) => (
        <div key={item.label} className="flex items-center gap-1.25">
          <span className="size-1.5 rounded-full" style={{ backgroundColor: getLineColor(item) }} />
          <span className="text-axis text-text-secondary">{item.label}</span>
        </div>
      ))}
    </div>
  );
}

export function getNiceMax(values: number[]) {
  const peak = Math.max(...values, 0);
  if (peak <= 0) {
    return 1;
  }
  const magnitude = 10 ** Math.floor(Math.log10(peak));
  const steps = [1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10];
  const step = steps.find((candidate) => candidate * magnitude >= peak) ?? 10;
  return step * magnitude;
}
