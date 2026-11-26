import { Icon } from '~/components/icons/icon';
import { MetricFilterChip } from '~/components/metrics/metric-filter-chip';
import { MetricFilterMenu } from '~/components/metrics/metric-filter-menu';
import { ChartLegend, getNiceDomain, MetricChart } from '~/components/servers/metric-chart';
import type { ChartSeries } from '~/components/servers/metric-chart';
import { Button } from '~/components/ui/button';
import { RadioMenu } from '~/components/ui/radio-menu';
import { Skeleton } from '~/components/ui/skeleton';
import type { ParamChanges } from '~/hooks/use-list-params';
import { cn } from '~/utils/classname';
import { pluralize } from '~/utils/format';
import {
  formatMetricValue,
  getAggregations,
  getDefaultAggregation,
  getHeadlineSeries,
  getLatestValue,
  METRIC_RANGES,
  writeMetricFilters,
} from '~/utils/metrics';
import type { MetricAggregation, MetricAttribute, MetricFilter, MetricResult, MetricSummary } from '~/utils/metrics';
import type { TimeRange } from '~/utils/time-range';

const CHART_HEIGHT = 156;

type MetricOverTimeProps = {
  metric: MetricSummary;
  result?: MetricResult;
  series: ChartSeries[];
  aggregation: MetricAggregation;
  range: TimeRange;
  groupBy?: string;
  filters: MetricFilter[];
  attributes?: MetricAttribute[];
  limit?: number;
  isBelow: boolean;
  isFiring: boolean;
  isError: boolean;
  isShowingOldResult: boolean;
  onRetry: () => void;
  onChange: (changes: ParamChanges) => void;
};

export function MetricOverTime(props: MetricOverTimeProps) {
  const {
    metric,
    result,
    series,
    aggregation,
    range,
    groupBy,
    filters,
    attributes,
    limit,
    isBelow,
    isFiring,
    isError,
    isShowingOldResult,
    onRetry,
    onChange,
  } = props;

  const aggregations = getAggregations(metric);
  const label = aggregations.find((item) => item.value === aggregation)?.label ?? aggregation;
  const formatValue = (value: number) => formatMetricValue(value, metric.unit, aggregation);
  const values = series.flatMap((line) => line.values.filter((value) => value !== null));
  const { min, max } = getNiceDomain([...values, ...(limit === undefined ? [] : [limit * 1.2])]);
  const to = Date.now();
  const from = result?.times[0] ?? to - range.ms;

  const groupOptions = [
    { value: '', label: 'No Grouping' },
    ...['service', 'server', ...(attributes ?? []).map((attribute) => attribute.key)].map((key) => ({
      value: key,
      label: <span className="font-mono text-code-small">{key}</span>,
    })),
  ];

  const headlineSeries = groupBy ? getHeadlineSeries(series, isBelow) : series[0];
  const headline = headlineSeries ? getLatestValue(headlineSeries.values) : undefined;

  const setFilters = (next: MetricFilter[]) => onChange({ filter: writeMetricFilters(next) });

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="flex-1 text-title-section font-medium text-text-primary">Over Time</h2>

        <RadioMenu
          icon="chart-line"
          size="xs"
          label={label}
          value={aggregation}
          options={aggregations}
          onChange={(value) => onChange({ agg: value === getDefaultAggregation(metric) ? null : value })}
        />

        <RadioMenu
          icon="layers"
          size="xs"
          label={groupBy ? `By ${groupBy}` : 'No Grouping'}
          value={groupBy ?? ''}
          options={groupOptions}
          onChange={(value) => onChange({ group: value || null })}
        />

        <MetricFilterMenu
          attributes={attributes}
          usedKeys={filters.map((filter) => filter.key)}
          onAdd={(key, value) => setFilters([...filters, { key, value, isNegated: false }])}
        />

        {filters.map((filter, index) => (
          <MetricFilterChip
            key={filter.key}
            filter={filter}
            onToggle={() =>
              setFilters(filters.map((item, other) => (other === index ? { ...item, isNegated: !item.isNegated } : item)))
            }
            onRemove={() => setFilters(filters.filter((_, other) => other !== index))}
          />
        ))}

        <RadioMenu
          icon="calendar"
          size="xs"
          label={range.label}
          value={range.value}
          options={METRIC_RANGES}
          onChange={(value) => onChange({ range: value === '1h' ? null : value })}
        />
      </div>

      {!result && isError && (
        <div
          role="alert"
          className="flex h-61 w-full flex-col items-center justify-center gap-2 rounded-2xl border border-border-ring bg-surface-default shadow-card"
        >
          <div className="flex items-center gap-2">
            <Icon name="triangle-alert" className="shrink-0 text-text-tertiary" />
            <p className="text-body text-text-secondary">Couldn’t load this chart</p>
          </div>
          <Button type="button" variant="secondary" size="xs" onClick={onRetry}>
            Try Again
          </Button>
        </div>
      )}

      {!result && !isError && <Skeleton size="block" className="h-61 w-full rounded-2xl" />}

      {result && (
        <div className={cn('transition-opacity', isShowingOldResult && 'opacity-60')}>
          <MetricChart
            title={label}
            value={headline === undefined ? '—' : formatValue(headline)}
            detail={groupBy ? headlineSeries?.label : pluralize(series.length, 'series', 'series')}
            isValueCritical={isFiring}
            series={series}
            times={result.times}
            from={from}
            to={to}
            bucketMs={result.step_ms}
            min={min}
            max={max}
            maxLabel={formatValue(max)}
            formatValue={formatValue}
            limit={limit}
            limitLabel={limit === undefined ? undefined : formatValue(limit)}
            height={CHART_HEIGHT}
            legend={
              series.length > 0 && (
                <ChartLegend
                  items={series.slice(0, 6).map((line) => {
                    const last = getLatestValue(line.values);
                    return {
                      label: last === undefined ? line.label : `${line.label} ${formatValue(last)}`,
                      color: line.color,
                    };
                  })}
                />
              )
            }
            footer={
              series.length === 0 && (
                <p className="text-body text-text-secondary">No points in this range. Try a longer range or fewer filters.</p>
              )
            }
          />
        </div>
      )}
      {result?.is_truncated && <p className="text-caption text-text-tertiary">Showing the 50 largest groups.</p>}
    </section>
  );
}
