import { useQuery } from '@tanstack/react-query';

import { Icon } from '~/components/icons/icon';
import { MetricExemplars } from '~/components/metrics/metric-exemplars';
import { MetricFilterChip } from '~/components/metrics/metric-filter-chip';
import { MetricFilterMenu } from '~/components/metrics/metric-filter-menu';
import { MetricGroupList } from '~/components/metrics/metric-group-list';
import { getNiceMax, MetricChart } from '~/components/servers/metric-chart';
import type { ChartSeries } from '~/components/servers/metric-chart';
import { Badge } from '~/components/ui/badge';
import { Button } from '~/components/ui/button';
import { Menu } from '~/components/ui/menu';
import { Skeleton } from '~/components/ui/skeleton';
import { metricQueries } from '~/queries/metrics';
import { pluralize } from '~/utils/format';
import {
  formatMetricValue,
  getAggregations,
  getGroupLabel,
  getKindLabel,
  getSeriesColor,
  writeMetricFilters,
} from '~/utils/metrics';
import type { MetricAggregation, MetricFilter, MetricSummary } from '~/utils/metrics';

const CHART_HEIGHT = 240;

type MetricViewProps = {
  metric: MetricSummary;
  rangeMs: number;
  service?: string;
  serverId?: string;
  aggregation?: MetricAggregation;
  groupBy?: string;
  filters: MetricFilter[];
  onChange: (changes: Record<string, string | null>) => void;
};

export function MetricView(props: MetricViewProps) {
  const { metric, rangeMs, service, serverId, aggregation, groupBy, filters, onChange } = props;

  const aggregations = getAggregations(metric);
  const picked = aggregations.find((item) => item.value === aggregation);
  const { data: attributes } = useQuery(metricQueries.attributes(metric.name));
  const { data: result, isLoading } = useQuery(
    metricQueries.query({
      name: metric.name,
      rangeMs,
      agg: picked?.value,
      service,
      server_id: serverId,
      filter: writeMetricFilters(filters) || undefined,
      group_by: groupBy,
    })
  );

  const shown = result?.name === metric.name ? result : undefined;
  const shownAggregation = shown?.aggregation ?? picked?.value ?? aggregations[0]!.value;
  const label = aggregations.find((item) => item.value === shownAggregation)?.label ?? shownAggregation;
  const formatValue = (value: number) => formatMetricValue(value, metric.unit, shownAggregation);
  const series: ChartSeries[] = (shown?.series ?? []).map((line, index) => ({
    label: getGroupLabel(line.group),
    values: line.values,
    color: getSeriesColor(index),
  }));
  const values = series.flatMap((line) => line.values.filter((value) => value !== null));
  const latest = series[0]?.values.filter((value) => value !== null).at(-1);
  const max = getNiceMax(values);
  const to = Date.now();
  const from = shown?.times[0] ?? to - rangeMs;
  const groupKeys = ['service', 'server', ...(attributes ?? []).map((attribute) => attribute.key)];

  const setFilters = (next: MetricFilter[]) => onChange({ filter: writeMetricFilters(next) || null });

  return (
    <section aria-label={metric.name} className="flex min-w-0 flex-1 flex-col gap-3">
      <div className="flex flex-col gap-1">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="truncate font-mono text-code text-text-primary">{metric.name}</h2>
          <Badge>{getKindLabel(metric)}</Badge>
          {metric.unit && <span className="text-caption text-text-tertiary">{metric.unit}</span>}
        </div>
        {metric.description && <p className="text-body text-text-secondary">{metric.description}</p>}
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <Menu.Root>
          <Menu.Trigger render={<Button variant="secondary" className="w-fit" />}>
            <Icon name="chart-line" />
            {label}
            <Icon name="chevron-down" />
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Positioner align="start">
              <Menu.Popup className="min-w-40">
                <Menu.RadioGroup
                  value={shownAggregation}
                  onValueChange={(value: string) =>
                    onChange({ agg: value === aggregations[0]!.value ? null : value })
                  }
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
          <Menu.Trigger render={<Button variant="secondary" className="w-fit" />}>
            <Icon name="layers" />
            {groupBy ? `By ${groupBy}` : 'No Grouping'}
            <Icon name="chevron-down" />
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Positioner align="start">
              <Menu.Popup className="max-h-80 min-w-40 overflow-y-auto">
                <Menu.RadioGroup
                  value={groupBy ?? ''}
                  onValueChange={(value: string) => onChange({ group: value || null })}
                >
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
      </div>

      {isLoading && !shown && <Skeleton size="block" className="h-82 w-full rounded-2xl" />}

      {shown && (
        <div className="flex flex-col gap-3">
          <MetricChart
            title={label}
            value={latest === undefined ? '—' : formatValue(latest)}
            detail={pluralize(series.length, 'series', 'series')}
            series={series}
            times={shown.times}
            from={from}
            to={to}
            bucketMs={shown.step_ms}
            max={max}
            maxLabel={formatValue(max)}
            formatValue={formatValue}
            height={CHART_HEIGHT}
            footer={<MetricExemplars exemplars={shown.exemplars} series={series} bucketMs={shown.step_ms} from={from} to={to} />}
          />
          {series.length === 0 && (
            <p className="text-body text-text-secondary">No points in this range. Try a longer range or fewer filters.</p>
          )}
          {groupBy && series.length > 0 && <MetricGroupList series={series} formatValue={formatValue} />}
          {shown.is_truncated && (
            <p className="text-caption text-text-tertiary">Showing the 50 largest groups.</p>
          )}
        </div>
      )}
    </section>
  );
}
