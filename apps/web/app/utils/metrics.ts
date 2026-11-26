import type { Schemas } from '@daroyan/client';

import { formatBytes, getUnitSuffix, HOUR_MS } from '~/utils/format';
import { getTimeRanges, readTimeRange } from '~/utils/time-range';
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

export const METRIC_RANGES = getTimeRanges(['1h', '6h', '24h', '7d', '30d']);

export function readMetricRange(value: string | null) {
  return readTimeRange(METRIC_RANGES, value);
}

type MetricKindSource = Pick<MetricSummary, 'kind' | 'is_monotonic'>;

export type MetricKind = 'histogram' | 'counter' | 'gauge';

export function getMetricKind(metric: MetricKindSource): MetricKind {
  if (metric.kind === 'histogram') {
    return 'histogram';
  }
  if (metric.kind === 'sum' && metric.is_monotonic) {
    return 'counter';
  }
  return 'gauge';
}

const COMPACT_FORMAT = new Intl.NumberFormat('en-US', { notation: 'compact', maximumFractionDigits: 1 });
const PLAIN_FORMAT = new Intl.NumberFormat('en-US', { maximumFractionDigits: 2 });

export function getAggregations(metric: MetricKindSource) {
  switch (getMetricKind(metric)) {
    case 'histogram':
      return HISTOGRAM_AGGREGATIONS;
    case 'counter':
      return COUNTER_AGGREGATIONS;
    case 'gauge':
      return GAUGE_AGGREGATIONS;
  }
}

export function getDefaultAggregation(metric: MetricKindSource): MetricAggregation {
  if (getMetricKind(metric) === 'histogram') {
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

export function getSpecReads(metric: MetricKindSource) {
  switch (getMetricKind(metric)) {
    case 'histogram':
      return HISTOGRAM_SPECS;
    case 'counter':
      return COUNTER_SPECS;
    case 'gauge':
      return GAUGE_SPECS;
  }
}

export function getSeriesColumns(metric: MetricKindSource) {
  switch (getMetricKind(metric)) {
    case 'histogram':
      return HISTOGRAM_COLUMNS;
    case 'counter':
      return COUNTER_COLUMNS;
    case 'gauge':
      return GAUGE_COLUMNS;
  }
}

export function getMetricLink(name: string) {
  return `/metrics/${encodeURIComponent(name)}`;
}

export function getKindLabel(metric: MetricKindSource) {
  switch (getMetricKind(metric)) {
    case 'histogram':
      return 'Histogram';
    case 'counter':
      return 'Counter';
    case 'gauge':
      return 'Gauge';
  }
}

const UNIT_WORDS: Record<string, string> = {
  '1': '',
  '%': '%',
  s: 's',
  ms: 'ms',
  us: 'µs',
  ns: 'ns',
  min: 'min',
  h: 'h',
  d: 'd',
  By: 'bytes',
  KiBy: 'KiB',
  MiBy: 'MiB',
  GiBy: 'GiB',
  TiBy: 'TiB',
  KBy: 'kB',
  MBy: 'MB',
  GBy: 'GB',
  bit: 'bits',
  'By/s': 'B/s',
  Cel: '°C',
};

const BYTE_SIZES: Record<string, number> = {
  By: 1,
  KiBy: 1_024,
  MiBy: 1_024 ** 2,
  GiBy: 1_024 ** 3,
  TiBy: 1_024 ** 4,
  KBy: 1_000,
  MBy: 1_000 ** 2,
  GBy: 1_000 ** 3,
};

const TIME_MICROS: Record<string, number> = {
  s: 1_000_000,
  ms: 1_000,
  us: 1,
  ns: 0.001,
};

export function getMetricUnitWord(unit: string) {
  const known = UNIT_WORDS[unit];
  if (known !== undefined) {
    return known;
  }

  const annotation = unit.match(/^\{(.+)\}$/)?.[1]?.trim();
  if (annotation) {
    return annotation.endsWith('s') ? annotation : `${annotation}s`;
  }

  return unit.replace(/[{}]/g, '').trim();
}

export function getMetricLimitUnit(unit: string, aggregation: MetricAggregation) {
  if (aggregation === 'count_rate') {
    return '/s';
  }

  const word = getMetricUnitWord(unit);
  if (aggregation === 'rate') {
    return `${word}/s`;
  }
  if (aggregation === 'rate_per_minute') {
    return `${word}/min`;
  }
  return word;
}

function formatNumber(value: number) {
  return Math.abs(value) >= 10_000 ? COMPACT_FORMAT.format(value) : PLAIN_FORMAT.format(value);
}

function formatMetricAmount(value: number, unit: string) {
  const micros = TIME_MICROS[unit];
  if (micros !== undefined && value >= 0) {
    return formatMicros(value * micros);
  }

  const bytes = BYTE_SIZES[unit];
  if (bytes !== undefined && value >= 0) {
    return formatBytes(value * bytes);
  }

  return `${formatNumber(value)}${getUnitSuffix(getMetricUnitWord(unit))}`;
}

export function formatMetricValue(value: number, unit: string, aggregation: MetricAggregation) {
  if (aggregation === 'count_rate') {
    return `${formatNumber(value)}/s`;
  }
  if (aggregation === 'rate') {
    return `${formatMetricAmount(value, unit)}/s`;
  }
  if (aggregation === 'rate_per_minute') {
    return `${formatMetricAmount(value, unit)}/min`;
  }
  return formatMetricAmount(value, unit);
}

export function getMetricTracesLink(metric: Pick<MetricSummary, 'services'>, service?: string) {
  const traceService = service ?? (metric.services.length === 1 ? metric.services[0] : undefined);
  return traceService ? `/traces?${new URLSearchParams({ service: traceService })}` : '/traces';
}

export function readMetricFilter(text: string): MetricFilter | undefined {
  const index = text.indexOf('=');
  if (index <= 0) {
    return undefined;
  }

  const isNegated = text[index - 1] === '!';
  const key = text.slice(0, isNegated ? index - 1 : index).trim();
  if (!key) {
    return undefined;
  }

  return { key, value: text.slice(index + 1).trim(), isNegated };
}

export function readMetricFilters(texts: string[]) {
  return texts.map(readMetricFilter).filter((filter) => filter !== undefined);
}

export function writeMetricFilter(filter: MetricFilter) {
  return `${filter.key}${filter.isNegated ? '!=' : '='}${filter.value}`;
}

export function writeMetricFilters(filters: MetricFilter[]) {
  return filters.map(writeMetricFilter);
}

type HeadlineSeries = {
  label: string;
  values: (number | null)[];
};

export function getLatestValue(values: (number | null)[]) {
  return values.filter((value) => value !== null).at(-1);
}

export function getHeadlineSeries<T extends HeadlineSeries>(series: T[], isBelow: boolean) {
  let headline: T | undefined;
  let headlineValue: number | undefined;

  for (const line of series) {
    const value = getLatestValue(line.values);
    if (value === undefined) {
      continue;
    }

    const isWorse = headlineValue === undefined || (isBelow ? value < headlineValue : value > headlineValue);
    if (isWorse) {
      headline = line;
      headlineValue = value;
    }
  }

  return headline;
}

export function getGroupLabel(group: string | null | undefined) {
  return group ?? 'All Series';
}

const SERIES_COLORS = ['var(--color-chart-primary)', 'var(--color-chart-secondary)', 'var(--color-chart-tertiary)'];

export function getSeriesColor(index: number) {
  return SERIES_COLORS[index] ?? `oklch(0.66 0.15 ${(250 + index * 137.5) % 360})`;
}
