import { useQueries } from '@tanstack/react-query';

import { DetailCard, DetailEmpty, DetailSection } from '~/components/servers/detail-table';
import type { ChartSeries } from '~/components/servers/metric-chart';
import { DataTable } from '~/components/ui/data-table';
import { useMetricSeriesTable } from '~/hooks/use-metric-series-table';
import type { MetricSeriesRow } from '~/hooks/use-metric-series-table';
import { metricQueries } from '~/queries/metrics';
import { pluralize } from '~/utils/format';
import { getGroupLabel, getSeriesColor, getSeriesColumns } from '~/utils/metrics';
import type { MetricSummary } from '~/utils/metrics';

type MetricSeriesSectionProps = {
  metric: MetricSummary;
  series: ChartSeries[];
  rangeMs: number;
  service?: string;
  serverId?: string;
  filter?: string;
  groupBy?: string;
};

export function MetricSeriesSection(props: MetricSeriesSectionProps) {
  const { metric, series, rangeMs, service, serverId, filter, groupBy } = props;

  const reads = getSeriesColumns(metric);
  const results = useQueries({
    queries: reads.map((read) =>
      metricQueries.query({
        name: metric.name,
        rangeMs,
        agg: read.aggregation,
        step_ms: rangeMs,
        service,
        server_id: serverId,
        filter,
        group_by: groupBy,
      })
    ),
  });

  const labels =
    series.length > 0
      ? series.map((line) => line.label)
      : (results[0]?.data?.series ?? []).map((line) => getGroupLabel(line.group));
  const rows: MetricSeriesRow[] = labels.map((label, index) => ({
    id: label,
    color: series.find((line) => line.label === label)?.color ?? getSeriesColor(index),
    values: results.map(
      (result) =>
        result.data?.series.find((line) => getGroupLabel(line.group) === label)?.values.at(-1) ?? undefined
    ),
  }));
  const table = useMetricSeriesTable({ rows, groupLabel: groupBy ?? 'Series', reads, unit: metric.unit });
  const isLoading = results.some((result) => result.isLoading);

  return (
    <DetailSection title="Series" summary={rows.length > 0 ? pluralize(rows.length, 'series', 'series') : undefined}>
      <DetailCard>
        {rows.length > 0 || isLoading ? (
          <DataTable table={table} label="Series" variant="card" size="lg" isLoading={isLoading} />
        ) : (
          <DetailEmpty>No series reported in this range.</DetailEmpty>
        )}
      </DetailCard>
    </DetailSection>
  );
}
