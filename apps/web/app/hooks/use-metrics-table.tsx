import { createColumnHelper } from '@tanstack/react-table';
import type { ColumnVisibilityState } from '@tanstack/react-table';
import { useMemo } from 'react';
import { Link } from 'react-router';

import { Badge } from '~/components/ui/badge';
import { Dash } from '~/components/ui/dash';
import { useDataTable } from '~/hooks/use-data-table';
import type { DataTableFeatures } from '~/lib/table';
import { formatCount, formatLogStamp, formatLogTime } from '~/utils/format';
import { getKindLabel, getMetricLink } from '~/utils/metrics';
import type { MetricSummary } from '~/utils/metrics';

type MetricRow = MetricSummary & { id: string };

const helper = createColumnHelper<DataTableFeatures, MetricRow>();

export const METRIC_COLUMN_LABELS: Record<string, string> = {
  description: 'Description',
  type: 'Type',
  services: 'Service',
  series: 'Series',
  last_seen_at: 'Last Seen',
};

function formatSeenTime(ms: number) {
  return new Date(ms).toDateString() === new Date().toDateString() ? formatLogTime(ms) : formatLogStamp(ms);
}

const columns = helper.columns([
  helper.accessor('name', {
    header: 'Metric',
    enableSorting: false,
    meta: { width: 316 },
    cell: ({ row }) => (
      <Link
        to={getMetricLink(row.original.name)}
        className="block truncate text-caption font-medium outline-none after:absolute after:inset-0 focus-visible:underline"
      >
        {row.original.name}
      </Link>
    ),
  }),
  helper.accessor('description', {
    header: 'Description',
    enableSorting: false,
    meta: { hideBelow: 'md' },
    cell: ({ getValue }) =>
      getValue() ? <span className="block truncate text-caption font-medium">{getValue()}</span> : <Dash />,
  }),
  helper.display({
    id: 'type',
    header: 'Type',
    meta: { width: 116, skeleton: 'badge' },
    cell: ({ row }) => <Badge>{getKindLabel(row.original)}</Badge>,
  }),
  helper.accessor('services', {
    header: 'Service',
    enableSorting: false,
    meta: { width: 176, hideBelow: 'lg' },
    cell: ({ getValue }) => (
      <span className="block truncate text-caption text-text-secondary">{getValue().join(', ') || '—'}</span>
    ),
  }),
  helper.accessor('series', {
    header: 'Series',
    enableSorting: false,
    meta: { width: 86, align: 'right', hideBelow: 'sm' },
    cell: ({ getValue }) => <span className="text-caption text-text-secondary tabular-nums">{formatCount(getValue())}</span>,
  }),
  helper.accessor('last_seen_at', {
    header: 'Last Seen',
    enableSorting: false,
    meta: { width: 126, align: 'right', hideBelow: 'sm' },
    cell: ({ getValue }) => (
      <span className="text-caption text-text-secondary tabular-nums">{formatSeenTime(getValue())}</span>
    ),
  }),
]);

type UseMetricsTableOptions = {
  metrics: MetricSummary[];
  columnVisibility: ColumnVisibilityState;
};

export function useMetricsTable(options: UseMetricsTableOptions) {
  const { metrics, columnVisibility } = options;

  const data = useMemo(() => metrics.map((metric) => ({ ...metric, id: metric.name })), [metrics]);

  return useDataTable({ columns, data, state: { columnVisibility } });
}
