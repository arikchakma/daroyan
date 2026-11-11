import type { Schemas } from '@daroyan/client';

import { formatBytes } from '~/utils/format';
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
  { value: 'count_rate', label: 'Count per Second' },
];

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

export function getSeriesColor(index: number) {
  if (index === 0) {
    return 'var(--color-chart-primary)';
  }
  return `oklch(0.66 0.15 ${(250 + index * 137.5) % 360})`;
}
