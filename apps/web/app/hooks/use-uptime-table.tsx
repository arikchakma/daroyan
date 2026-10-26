import type { Schemas } from '@daroyan/client';
import { createColumnHelper } from '@tanstack/react-table';
import type { SortingState } from '@tanstack/react-table';
import { useMemo } from 'react';
import { Link } from 'react-router';

import { UptimeBars } from '~/components/uptime/uptime-bars';
import { UptimeStatusBadge } from '~/components/uptime/uptime-status-badge';
import { SortableHeader } from '~/components/ui/sortable-header';
import { useDataTable } from '~/hooks/use-data-table';
import { resolveUpdater } from '~/lib/table';
import type { DataTableFeatures } from '~/lib/table';
import { cn } from '~/utils/classname';
import { formatDate, formatRelative } from '~/utils/format';
import { describeTarget, formatEvery, formatMs, formatUptime } from '~/utils/uptime';

type UptimeCheck = Schemas['UptimeCheck'];

const helper = createColumnHelper<DataTableFeatures, UptimeCheck>();

function isActive(check: UptimeCheck) {
  return !check.paused && check.status !== 'pending';
}

type UseUptimeTableOptions = {
  checks: UptimeCheck[];
  sorting: SortingState;
  onSortingChange: (sorting: SortingState) => void;
};

export function useUptimeTable(options: UseUptimeTableOptions) {
  const { checks, sorting, onSortingChange } = options;

  const columns = useMemo(
    () =>
      helper.columns([
        helper.accessor('name', {
          id: 'name',
          header: ({ column }) => <SortableHeader column={column} label="Check" />,
          sortFn: 'text',
          sortDescFirst: false,
          meta: { skeleton: 'lines' },
          cell: ({ row }) => (
            <span className="flex min-w-0 flex-col gap-0.5">
              <Link
                to={`/uptime/${row.original.id}`}
                className="truncate font-medium outline-none after:absolute after:inset-0 focus-visible:underline"
              >
                {row.original.name}
              </Link>
              <span className="truncate font-mono text-code-small text-text-tertiary">
                {describeTarget(row.original)}
              </span>
            </span>
          ),
        }),
        helper.accessor('status', {
          header: 'Status',
          enableSorting: false,
          meta: { width: 120, skeleton: 'badge' },
          cell: ({ getValue }) => <UptimeStatusBadge status={getValue()} />,
        }),
        helper.display({
          id: 'days',
          header: 'Last 30 Days',
          meta: { width: 170, hideBelow: 'lg' },
          cell: ({ row }) => (
            <UptimeBars days={row.original.days} className={cn(row.original.paused && 'opacity-40')} />
          ),
        }),
        helper.accessor((check) => (isActive(check) ? (check.uptime ?? -1) : -1), {
          id: 'uptime',
          header: ({ column }) => <SortableHeader column={column} label="Uptime" />,
          sortFn: 'basic',
          sortDescFirst: false,
          meta: { width: 100, align: 'right' },
          cell: ({ row }) => (isActive(row.original) ? formatUptime(row.original.uptime) : <Dash />),
        }),
        helper.accessor((check) => (isActive(check) ? (check.last_response_ms ?? -1) : -1), {
          id: 'response',
          header: ({ column }) => <SortableHeader column={column} label="Response" />,
          sortFn: 'basic',
          sortDescFirst: true,
          meta: { width: 110, align: 'right', hideBelow: 'md' },
          cell: ({ row }) =>
            isActive(row.original) && row.original.last_response_ms !== null && row.original.last_response_ms !== undefined ? (
              <span className={cn(row.original.status === 'degraded' && 'text-tone-warning-fg')}>
                {formatMs(row.original.last_response_ms)}
              </span>
            ) : (
              <Dash />
            ),
        }),
        helper.accessor('interval_secs', {
          header: 'Every',
          enableSorting: false,
          meta: { width: 80, align: 'right', hideBelow: 'md' },
          cell: ({ getValue }) => formatEvery(getValue()),
        }),
        helper.accessor((check) => check.last_checked_at ?? 0, {
          id: 'lastCheck',
          header: ({ column }) => <SortableHeader column={column} label="Last Check" />,
          sortFn: 'basic',
          sortDescFirst: true,
          meta: { width: 140, align: 'right' },
          cell: ({ row }) => {
            const at = row.original.last_checked_at;
            return (
              <span className="text-text-secondary">
                {!at ? 'Never' : row.original.paused ? formatDate(at) : formatRelative(at)}
              </span>
            );
          },
        }),
      ]),
    []
  );

  return useDataTable({
    columns,
    data: checks,
    state: { sorting },
    onSortingChange: (updater) => onSortingChange(resolveUpdater(updater, sorting)),
  });
}

function Dash() {
  return <span className="text-text-tertiary">—</span>;
}
