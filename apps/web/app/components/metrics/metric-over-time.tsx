import { Icon } from '~/components/icons/icon';
import { MetricFilterChip } from '~/components/metrics/metric-filter-chip';
import { MetricFilterMenu } from '~/components/metrics/metric-filter-menu';
import { ChartLegend, getNiceMax, MetricChart } from '~/components/servers/metric-chart';
import type { ChartSeries } from '~/components/servers/metric-chart';
import { Button } from '~/components/ui/button';
import { Menu } from '~/components/ui/menu';
import { RadioMenu } from '~/components/ui/radio-menu';
import { Skeleton } from '~/components/ui/skeleton';
import { pluralize } from '~/utils/format';
import { formatMetricValue, getAggregations, getDefaultAggregation, METRIC_RANGES, writeMetricFilters } from '~/utils/metrics';
import type { MetricAggregation, MetricAttribute, MetricFilter, MetricRange, MetricResult, MetricSummary } from '~/utils/metrics';

const CHART_HEIGHT = 156;

type MetricOverTimeProps = {
  metric: MetricSummary;
  result?: MetricResult;
  series: ChartSeries[];
  aggregation: MetricAggregation;
  range: MetricRange;
  groupBy?: string;
  filters: MetricFilter[];
  attributes?: MetricAttribute[];
  limit?: number;
  isFiring: boolean;
  onChange: (changes: Record<string, string | null>) => void;
};

export function MetricOverTime(props: MetricOverTimeProps) {
  const { metric, result, series, aggregation, range, groupBy, filters, attributes, limit, isFiring, onChange } = props;

  const aggregations = getAggregations(metric);
  const label = aggregations.find((item) => item.value === aggregation)?.label ?? aggregation;
  const formatValue = (value: number) => formatMetricValue(value, metric.unit, aggregation);
  const latest = (line?: ChartSeries) => line?.values.filter((value) => value !== null).at(-1);
  const values = series.flatMap((line) => line.values.filter((value) => value !== null));
  const max = getNiceMax([...values, ...(limit === undefined ? [] : [limit * 1.2])]);
  const to = Date.now();
  const from = result?.times[0] ?? to - range.ms;
  const groupKeys = ['service', 'server', ...(attributes ?? []).map((attribute) => attribute.key)];
  const headline = latest(series[0]);

  const setFilters = (next: MetricFilter[]) => onChange({ filter: writeMetricFilters(next) || null });

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="flex-1 text-title-section font-medium text-text-primary">Over Time</h2>

        <Menu.Root>
          <Menu.Trigger render={<Button variant="secondary" size="xs" />}>
            <Icon name="chart-line" />
            {label}
            <Icon name="chevron-down" />
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Positioner align="end">
              <Menu.Popup className="min-w-40">
                <Menu.RadioGroup
                  value={aggregation}
                  onValueChange={(value: string) => onChange({ agg: value === getDefaultAggregation(metric) ? null : value })}
                >
                  {aggregations.map((item) => (
                    <Menu.RadioItem key={item.value} value={item.value}>
                      {item.label}
                    </Menu.RadioItem>
                  ))}
                </Menu.RadioGroup>
              </Menu.Popup>
            </Menu.Positioner>
          </Menu.Portal>
        </Menu.Root>

        <Menu.Root>
          <Menu.Trigger render={<Button variant="secondary" size="xs" />}>
            <Icon name="layers" />
            {groupBy ? `By ${groupBy}` : 'No Grouping'}
            <Icon name="chevron-down" />
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Positioner align="end">
              <Menu.Popup className="max-h-80 min-w-40 overflow-y-auto">
                <Menu.RadioGroup value={groupBy ?? ''} onValueChange={(value: string) => onChange({ group: value || null })}>
                  <Menu.RadioItem value="">No Grouping</Menu.RadioItem>
                  {groupKeys.map((key) => (
                    <Menu.RadioItem key={key} value={key}>
                      <span className="font-mono text-code-small">{key}</span>
                    </Menu.RadioItem>
                  ))}
                </Menu.RadioGroup>
              </Menu.Popup>
            </Menu.Positioner>
          </Menu.Portal>
        </Menu.Root>

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
          options={[...METRIC_RANGES]}
          onChange={(value) => onChange({ range: value === '1h' ? null : value })}
        />
      </div>

      {!result && <Skeleton size="block" className="h-61 w-full rounded-2xl" />}

      {result && (
        <MetricChart
          title={label}
          value={headline === undefined || headline === null ? '—' : formatValue(headline)}
          detail={pluralize(series.length, 'series', 'series')}
          isValueCritical={isFiring}
          series={series}
          times={result.times}
          from={from}
          to={to}
          bucketMs={result.step_ms}
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
                  const last = latest(line);
                  return {
                    label: last === undefined || last === null ? line.label : `${line.label} ${formatValue(last)}`,
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
      )}
      {result?.is_truncated && <p className="text-caption text-text-tertiary">Showing the 50 largest groups.</p>}
    </section>
  );
}
