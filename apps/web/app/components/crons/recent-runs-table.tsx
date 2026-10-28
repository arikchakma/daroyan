import type { Schemas } from '@daroyan/client';
import { createColumnHelper } from '@tanstack/react-table';
import { useMemo } from 'react';

import { CronStatusBadge } from '~/components/crons/cron-status-badge';
import { DataTable } from '~/components/ui/data-table';
import { useDataTable } from '~/hooks/use-data-table';
import type { DataTableFeatures } from '~/lib/table';
import { formatRunTime } from '~/utils/crons';

const RUN_SECOND_FORMAT = new Intl.DateTimeFormat('en-GB', { hour: '2-digit', minute: '2-digit', second: '2-digit' });

type RunRow = Schemas['CronRun'] & { id: string };

const runHelper = createColumnHelper<DataTableFeatures, RunRow>();

type RecentRunsTableProps = {
  runs: Schemas['CronRun'][];
  serverName?: string | null;
  onOpen: (run: Schemas['CronRun']) => void;
};

export function RecentRunsTable(props: RecentRunsTableProps) {
  const { runs, serverName, onOpen } = props;

  const rows = useMemo(() => runs.map((run, index) => ({ ...run, id: `${run.finished_at}-${index}` })), [runs]);
  const columns = useMemo(
    () =>
      runHelper.columns([
        runHelper.display({
          id: 'started',
          header: 'Started',
          meta: { width: 120 },
          cell: ({ row }) => (
            <button
              type="button"
              onClick={() => onOpen(runs[row.index] ?? row.original)}
              className="block w-full text-left font-mono text-code-small outline-none after:absolute after:inset-0 focus-visible:underline"
            >
              {RUN_SECOND_FORMAT.format(new Date(row.original.started_at ?? row.original.scheduled_at ?? row.original.finished_at))}
            </button>
          ),
        }),
        runHelper.display({
          id: 'server',
          header: 'Server',
          cell: () => <span className="block truncate">{serverName ?? '—'}</span>,
        }),
        runHelper.accessor('status', {
          header: 'Status',
          enableSorting: false,
          meta: { width: 120 },
          cell: ({ getValue }) => <CronStatusBadge status={getValue()} />,
        }),
        runHelper.accessor('exit_code', {
          header: 'Exit',
          enableSorting: false,
          meta: { width: 80 },
          cell: ({ getValue }) => <span className="font-mono text-code-small">{getValue() ?? '—'}</span>,
        }),
        runHelper.accessor('duration_ms', {
          header: 'Duration',
          enableSorting: false,
          meta: { width: 100, align: 'right' },
          cell: ({ getValue }) => formatRunTime(getValue()),
        }),
      ]),
    [runs, serverName, onOpen]
  );
  const table = useDataTable({ columns, data: rows });

  return <DataTable table={table} label="Recent Runs" variant="card" size="sm" />;
}
