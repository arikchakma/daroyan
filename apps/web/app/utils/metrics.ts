import type { Schemas } from '@daroyan/client';

import { DAY_MS, formatBytes, HOUR_MS } from '~/utils/format';
import { formatMicros } from '~/utils/traces';

export const METRICS_TARGET_ID = 'metrics';
export const METRICS_TARGET_NAME = 'Metrics';

export type MetricSummary = Schemas['MetricSummary'];
export type MetricResult = Schemas['MetricResult'];
export type MetricAggregation = Schemas['MetricAggregation'];
export type MetricAttribute = Schemas['MetricAttribute'];

export type AggregationOption = {
  value: MetricAggregation;
  label: string;
};

export type MetricFilter = {
  key: string;
  value: string;
  isNegated: boolean;
};

const COUNTER_AGGREGATIONS: AggregationOption[] = [
  { value: 'rate_per_minute', label: 'Rate per Minute' },
  { value: 'rate', label: 'Rate per Second' },
  { value: 'increase', label: 'Increase' },
];

const GAUGE_AGGREGATIONS: AggregationOption[] = [
  { value: 'last', label: 'Last Value' },
  { value: 'avg', label: 'Average' },
  { value: 'min', label: 'Minimum' },
  { value: 'max', label: 'Maximum' },
];

const HISTOGRAM_AGGREGATIONS: AggregationOption[] = [
  { value: 'p50', label: 'p50' },
  { value: 'p90', label: 'p90' },
  { value: 'p95', label: 'p95' },
  { value: 'p99', label: 'p99' },
  { value: 'avg', label: 'Average' },
  { value: 'min', label: 'Minimum' },
  { value: 'max', label: 'Maximum' },
  { value: 'count_rate', label: 'Count per Second' },
];

export type MetricRead = {
  label: string;
  aggregation: MetricAggregation;
};

export type MetricSpecRead = MetricRead & {
  detail: string;
  windowMs: number;
};

const SPEC_WINDOW_MS = 300_000;

const HISTOGRAM_SPECS: MetricSpecRead[] = [
  { label: 'p50', aggregation: 'p50', detail: 'last 5m', windowMs: SPEC_WINDOW_MS },
  { label: 'p95', aggregation: 'p95', detail: 'last 5m', windowMs: SPEC_WINDOW_MS },
  { label: 'p99', aggregation: 'p99', detail: 'last 5m', windowMs: SPEC_WINDOW_MS },
  { label: 'Requests', aggregation: 'count_rate', detail: 'last 5m', windowMs: SPEC_WINDOW_MS },
];

const COUNTER_SPECS: MetricSpecRead[] = [
  { label: 'Per Minute', aggregation: 'rate_per_minute', detail: 'last 5m', windowMs: SPEC_WINDOW_MS },
  { label: 'Per Second', aggregation: 'rate', detail: 'last 5m', windowMs: SPEC_WINDOW_MS },
  { label: 'Increase', aggregation: 'increase', detail: 'last 5m', windowMs: SPEC_WINDOW_MS },
  { label: 'Increase', aggregation: 'increase', detail: 'last hour', windowMs: HOUR_MS },
];

const GAUGE_SPECS: MetricSpecRead[] = [
  { label: 'Last Value', aggregation: 'last', detail: 'now', windowMs: SPEC_WINDOW_MS },
  { label: 'Average', aggregation: 'avg', detail: 'last 5m', windowMs: SPEC_WINDOW_MS },
  { label: 'Maximum', aggregation: 'max', detail: 'last 5m', windowMs: SPEC_WINDOW_MS },
  { label: 'Minimum', aggregation: 'min', detail: 'last 5m', windowMs: SPEC_WINDOW_MS },
];

const HISTOGRAM_COLUMNS: MetricRead[] = [
  { label: 'Requests', aggregation: 'count_rate' },
  { label: 'Min', aggregation: 'min' },
  { label: 'p50', aggregation: 'p50' },
  { label: 'p95', aggregation: 'p95' },
  { label: 'p99', aggregation: 'p99' },
  { label: 'Max', aggregation: 'max' },
];

const COUNTER_COLUMNS: MetricRead[] = [
  { label: 'Per Minute', aggregation: 'rate_per_minute' },
  { label: 'Increase', aggregation: 'increase' },
];

const GAUGE_COLUMNS: MetricRead[] = [
  { label: 'Last', aggregation: 'last' },
  { label: 'Min', aggregation: 'min' },
  { label: 'Average', aggregation: 'avg' },
  { label: 'Max', aggregation: 'max' },
];

export const METRIC_RANGES = [
  { value: '1h', label: 'Last Hour', ms: HOUR_MS },
  { value: '6h', label: 'Last 6 Hours', ms: 6 * HOUR_MS },
  { value: '24h', label: 'Last 24 Hours', ms: DAY_MS },
  { value: '7d', label: 'Last 7 Days', ms: 7 * DAY_MS },
  { value: '30d', label: 'Last 30 Days', ms: 30 * DAY_MS },
] as const;

export type MetricRange = (typeof METRIC_RANGES)[number];

export function readMetricRange(value: string | null): MetricRange {
  return METRIC_RANGES.find((item) => item.value === value) ?? METRIC_RANGES[0];
}

const COMPACT_FORMAT = new Intl.NumberFormat('en-US', { notation: 'compact', maximumFractionDigits: 1 });
const PLAIN_FORMAT = new Intl.NumberFormat('en-US', { maximumFractionDigits: 2 });

export function getAggregations(metric: Pick<MetricSummary, 'kind' | 'is_monotonic'>) {
  if (metric.kind === 'histogram') {
    return HISTOGRAM_AGGREGATIONS;
  }
  if (metric.kind === 'sum' && metric.is_monotonic) {
    return COUNTER_AGGREGATIONS;
  }
  return GAUGE_AGGREGATIONS;
}

export function getDefaultAggregation(metric: Pick<MetricSummary, 'kind' | 'is_monotonic'>): MetricAggregation {
  if (metric.kind === 'histogram') {
    return 'p95';
  }
  return getAggregations(metric)[0]!.value;
}

export function getMetricUnitLabel(unit: string) {
  if (unit === 's') {
    return 'seconds';
  }
  if (unit === 'ms') {
    return 'milliseconds';
  }
  if (unit === 'us') {
    return 'microseconds';
  }
  if (unit === 'By') {
    return 'bytes';
  }
  if (unit === '1') {
    return 'ratio';
  }
  return getMetricUnitWord(unit) || unit;
}

export function getSpecReads(metric: Pick<MetricSummary, 'kind' | 'is_monotonic'>) {
  if (metric.kind === 'histogram') {
    return HISTOGRAM_SPECS;
  }
  if (metric.kind === 'sum' && metric.is_monotonic) {
    return COUNTER_SPECS;
  }
  return GAUGE_SPECS;
}

export function getSeriesColumns(metric: Pick<MetricSummary, 'kind' | 'is_monotonic'>) {
  if (metric.kind === 'histogram') {
    return HISTOGRAM_COLUMNS;
  }
  if (metric.kind === 'sum' && metric.is_monotonic) {
    return COUNTER_COLUMNS;
  }
  return GAUGE_COLUMNS;
}

export function getMetricLink(name: string) {
  return `/metrics/${encodeURIComponent(name)}`;
}

export function getKindLabel(metric: Pick<MetricSummary, 'kind' | 'is_monotonic'>) {
  if (metric.kind === 'histogram') {
    return 'Histogram';
  }
  if (metric.kind === 'sum' && metric.is_monotonic) {
    return 'Counter';
  }
  return 'Gauge';
}

export function getMetricUnitWord(unit: string) {
  const word = unit.replace(/[{}]/g, '').trim();
  if (!word || word === '1') {
    return '';
  }
  return word.endsWith('s') ? word : `${word}s`;
}

function formatNumber(value: number) {
  return Math.abs(value) >= 10_000 ? COMPACT_FORMAT.format(value) : PLAIN_FORMAT.format(value);
}

function formatAmount(value: number, unit: string) {
  if (unit === 's') {
    return formatMicros(value * 1_000_000);
  }
  if (unit === 'ms') {
    return formatMicros(value * 1_000);
  }
  if (unit === 'us') {
    return formatMicros(value);
  }
  if (unit === 'By') {
    return formatBytes(value);
  }
  if (unit === '1') {
    return `${formatNumber(value * 100)}%`;
  }
  if (unit === '%') {
    return `${formatNumber(value)}%`;
  }

  const word = getMetricUnitWord(unit);
  return word ? `${formatNumber(value)} ${word}` : formatNumber(value);
}

export function formatMetricValue(value: number, unit: string, aggregation: MetricAggregation) {
  if (aggregation === 'rate_per_minute') {
    return `${formatAmount(value, unit)}/min`;
  }
  if (aggregation === 'rate' || aggregation === 'count_rate') {
    const amount = aggregation === 'count_rate' ? formatNumber(value) : formatAmount(value, unit);
    return `${amount}/s`;
  }
  return formatAmount(value, unit);
}

export function readMetricFilters(text: string): MetricFilter[] {
  return text
    .split(',')
    .map((part) => {
      const negated = part.split('!=');
      if (negated.length === 2) {
        return { key: negated[0]!.trim(), value: negated[1]!.trim(), isNegated: true };
      }
      const [key = '', ...rest] = part.split('=');
      return { key: key.trim(), value: rest.join('=').trim(), isNegated: false };
    })
    .filter((filter) => filter.key);
}

export function writeMetricFilters(filters: MetricFilter[]) {
  return filters.map((filter) => `${filter.key}${filter.isNegated ? '!=' : '='}${filter.value}`).join(',');
}

export function getGroupLabel(group: string | null | undefined) {
  return group ?? 'All Series';
}

const SERIES_COLORS = ['var(--color-chart-primary)', 'var(--color-chart-secondary)', 'var(--color-chart-tertiary)'];

export function getSeriesColor(index: number) {
  return SERIES_COLORS[index] ?? `oklch(0.66 0.15 ${(250 + index * 137.5) % 360})`;
}

export function getMetricRuleText(condition: { threshold: number; below?: boolean }, forSecs: number) {
  const side = `${condition.below ? 'below' : 'above'} ${PLAIN_FORMAT.format(condition.threshold)}`;
  if (forSecs <= 0) {
    return side;
  }
  if (forSecs % 3_600 === 0) {
    return `${side} for ${forSecs / 3_600}h`;
  }
  return `${side} for ${Math.round(forSecs / 60)}m`;
}
