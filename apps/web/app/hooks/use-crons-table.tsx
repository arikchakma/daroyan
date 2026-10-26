import type { Schemas } from '@daroyan/client';
import { createColumnHelper } from '@tanstack/react-table';
import type { SortingState } from '@tanstack/react-table';
import { useMemo } from 'react';
import { Link } from 'react-router';

import { CronRunBars } from '~/components/crons/cron-run-bars';
import { CronStatusBadge } from '~/components/crons/cron-status-badge';
import { SortableHeader } from '~/components/ui/sortable-header';
import { useDataTable } from '~/hooks/use-data-table';
import { resolveUpdater } from '~/lib/table';
import type { DataTableFeatures } from '~/lib/table';
import { cn } from '~/utils/classname';
import { describeSchedule, formatRunTime, formatUntil } from '~/utils/crons';
import { formatDate, formatRelative } from '~/utils/format';

export type CronRow = Schemas['Cron'] & { mutedUntil?: string };

const helper = createColumnHelper<DataTableFeatures, CronRow>();

const LIST_RUNS = 12;

type UseCronsTableOptions = {
  crons: CronRow[];
  sorting: SortingState;
  onSortingChange: (sorting: SortingState) => void;
};

function describeJob(cron: CronRow) {
  return [describeSchedule(cron), cron.server_name, cron.mutedUntil && `muted ${cron.mutedUntil}`]
    .filter(Boolean)
    .join(' · ');
}

export function useCronsTable(options: UseCronsTableOptions) {
  const { crons, sorting, onSortingChange } = options;

  const columns = useMemo(
    () =>
      helper.columns([
        helper.accessor('name', {
          id: 'name',
          header: ({ column }) => <SortableHeader column={column} label="Job" />,
          sortFn: 'text',
          sortDescFirst: false,
          meta: { skeleton: 'lines' },
          cell: ({ row }) => (
            <span className="flex min-w-0 flex-col gap-0.5">
              <Link
                to={`/crons/${row.original.id}`}
                className="truncate font-mono text-code outline-none after:absolute after:inset-0 focus-visible:underline"
              >
                {row.original.name}
              </Link>
              <span className="truncate font-mono text-code-small text-text-tertiary">{describeJob(row.original)}</span>
            </span>
          ),
        }),
        helper.display({
          id: 'status',
          header: 'Status',
          meta: { width: 120, skeleton: 'badge' },
          cell: ({ row }) => <CronStatusBadge status={row.original.mutedUntil ? 'muted' : row.original.status} />,
        }),
        helper.display({
          id: 'runs',
          header: `Last ${LIST_RUNS} Runs`,
          meta: { width: 170, hideBelow: 'lg' },
          cell: ({ row }) => (
            <CronRunBars
              runs={row.original.runs}
              count={LIST_RUNS}
              barClassName="w-1 shrink-0"
              className={cn('h-4', row.original.paused && 'opacity-40')}
            />
          ),
        }),
        helper.accessor((cron) => cron.last_duration_ms ?? -1, {
          id: 'duration',
          header: ({ column }) => <SortableHeader column={column} label="Duration" />,
          sortFn: 'basic',
          sortDescFirst: true,
          meta: { width: 110, align: 'right', hideBelow: 'md' },
          cell: ({ row }) =>
            row.original.last_duration_ms === null || row.original.last_duration_ms === undefined || row.original.mutedUntil ? (
              <Dash />
            ) : (
              formatRunTime(row.original.last_duration_ms)
            ),
        }),
        helper.accessor((cron) => cron.last_ping_at ?? 0, {
          id: 'lastPing',
          header: ({ column }) => <SortableHeader column={column} label="Last Ping" />,
          sortFn: 'basic',
          sortDescFirst: true,
          meta: { width: 140, align: 'right' },
          cell: ({ row }) => {
            const at = row.original.last_ping_at;
            return (
              <span className="text-text-secondary">
                {!at ? 'Never' : row.original.paused ? formatDate(at) : formatRelative(at)}
              </span>
            );
          },
        }),
        helper.accessor((cron) => (cron.paused ? Number.MAX_SAFE_INTEGER : (cron.next_expected_at ?? Number.MAX_SAFE_INTEGER)), {
          id: 'next',
          header: ({ column }) => <SortableHeader column={column} label="Next Expected" />,
          sortFn: 'basic',
          sortDescFirst: false,
          meta: { width: 140, align: 'right', hideBelow: 'md' },
          cell: ({ row }) => <NextExpected cron={row.original} />,
        }),
      ]),
    []
  );

  return useDataTable({
    columns,
    data: crons,
    state: { sorting },
    onSortingChange: (updater) => onSortingChange(resolveUpdater(updater, sorting)),
  });
}

type NextExpectedProps = {
  cron: Schemas['Cron'];
};

export function NextExpected(props: NextExpectedProps) {
  const { cron } = props;

  if (cron.paused || cron.status === 'running' || !cron.next_expected_at) {
    return <Dash />;
  }
  if (cron.next_expected_at <= Date.now()) {
    return <span className="text-text-primary">Overdue</span>;
  }
  return <span className="text-text-secondary">{formatUntil(cron.next_expected_at)}</span>;
}

function Dash() {
  return <span className="text-text-tertiary">—</span>;
}
