import { useQuery } from '@tanstack/react-query';
import { createColumnHelper } from '@tanstack/react-table';
import type { ColumnVisibilityState, SortingState } from '@tanstack/react-table';
import { useMemo } from 'react';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';
import { useDataTable } from '~/hooks/use-data-table';
import type { DataTableFeatures } from '~/lib/table';
import { listServersOptions } from '~/queries/servers';
import { cn } from '~/utils/classname';
import { formatLogStamp, formatLogTime } from '~/utils/format';
import { describeSpans, formatMicros, getServiceColor, getTraceStatus } from '~/utils/traces';
import type { TraceSummary } from '~/utils/traces';

type TraceRow = TraceSummary & { id: string };

const helper = createColumnHelper<DataTableFeatures, TraceRow>();

export const TRACE_COLUMN_LABELS: Record<string, string> = {
  spans: 'Spans',
  status: 'Status',
  server: 'Server',
  duration_us: 'Duration',
  started_at: 'Time',
};

export function ServiceDot(props: { service: string; color?: string; className?: string }) {
  const { service, color, className } = props;

  return (
    <span
      aria-hidden="true"
      className={cn('inline-block size-1.5 shrink-0 rounded-full', className)}
      style={{ backgroundColor: color ?? getServiceColor(service) }}
    />
  );
}

type TraceServerProps = {
  trace: TraceSummary;
};

function TraceServer(props: TraceServerProps) {
  const { trace } = props;

  const { data: servers } = useQuery(listServersOptions());
  const server = servers?.find((item) => item.id === trace.server_id);
  if (!server) {
    return <span className="truncate">{trace.host ?? '—'}</span>;
  }

  return (
    <Link to={`/servers/${server.id}`} className="relative z-1 truncate outline-none hover:underline focus-visible:underline">
      {server.name}
    </Link>
  );
}

function formatTraceTime(ms: number) {
  return new Date(ms).toDateString() === new Date().toDateString() ? formatLogTime(ms) : formatLogStamp(ms);
}

const columns = helper.columns([
  helper.accessor('name', {
    header: 'Trace',
    enableSorting: false,
    meta: { width: 316 },
    cell: ({ row }) => (
      <Link
        to={`/traces/${row.original.trace_id}`}
        className="block truncate text-caption font-medium outline-none after:absolute after:inset-0 focus-visible:underline"
      >
        {row.original.name}
      </Link>
    ),
  }),
  helper.display({
    id: 'spans',
    header: 'Spans',
    meta: { hideBelow: 'md' },
    cell: ({ row }) => <span className="block truncate text-caption font-medium">{describeSpans(row.original)}</span>,
  }),
  helper.display({
    id: 'status',
    header: 'Status',
    meta: { width: 116, skeleton: 'badge' },
    cell: ({ row }) => {
      const status = getTraceStatus(row.original);
      return <Badge tone={status.tone}>{status.label}</Badge>;
    },
  }),
  helper.display({
    id: 'server',
    header: 'Server',
    meta: { width: 156, hideBelow: 'lg' },
    cell: ({ row }) => (
      <span className="flex min-w-0 items-center gap-1.5 text-caption text-text-secondary">
        <Icon name="server" className="size-3.5 shrink-0" />
        <TraceServer trace={row.original} />
      </span>
    ),
  }),
  helper.accessor('duration_us', {
    header: 'Duration',
    sortFn: 'basic',
    meta: { width: 106, align: 'right' },
    cell: ({ row }) => {
      const status = getTraceStatus(row.original);
      return (
        <span
          className={cn(
            'text-caption',
            status.tone === 'critical'
              ? 'text-tone-critical-fg'
              : status.isSlow
                ? 'text-tone-warning-fg'
                : 'text-text-secondary'
          )}
        >
          {formatMicros(row.original.duration_us)}
        </span>
      );
    },
  }),
  helper.accessor('started_at', {
    header: 'Time',
    sortFn: 'basic',
    meta: { width: 126, align: 'right', hideBelow: 'sm' },
    cell: ({ getValue }) => (
      <span className="text-caption text-text-secondary tabular-nums">{formatTraceTime(getValue() / 1_000)}</span>
    ),
  }),
]);

type UseTracesTableOptions = {
  traces: TraceSummary[];
  sorting: SortingState;
  columnVisibility: ColumnVisibilityState;
};

export function useTracesTable(options: UseTracesTableOptions) {
  const { traces, sorting, columnVisibility } = options;

  const data = useMemo(() => traces.map((trace) => ({ ...trace, id: trace.trace_id })), [traces]);

  return useDataTable({ columns, data, state: { sorting, columnVisibility } });
}
