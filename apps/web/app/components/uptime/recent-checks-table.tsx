import type { Schemas } from '@daroyan/client';
import { createColumnHelper } from '@tanstack/react-table';
import { useMemo } from 'react';

import { DataTable } from '~/components/ui/data-table';
import { UptimeStatusBadge } from '~/components/uptime/uptime-status-badge';
import { useDataTable } from '~/hooks/use-data-table';
import type { DataTableFeatures } from '~/lib/table';
import { formatEventTime } from '~/utils/format';
import { formatMs } from '~/utils/uptime';

function describeTimings(result: Schemas['UptimeResult']) {
  return [
    ['DNS', result.dns_ms],
    ['Connect', result.connect_ms],
    ['TLS', result.tls_ms],
    ['First byte', result.ttfb_ms],
  ]
    .flatMap(([label, ms]) => (ms === null || ms === undefined ? [] : [`${label} ${formatMs(ms as number)}`]))
    .join(' · ');
}

type ResultRow = Schemas['UptimeResult'] & { id: string };

const resultHelper = createColumnHelper<DataTableFeatures, ResultRow>();

type RecentChecksTableProps = {
  results: Schemas['UptimeResult'][];
};

export function RecentChecksTable(props: RecentChecksTableProps) {
  const { results } = props;

  const columns = useMemo(
    () =>
      resultHelper.columns([
        resultHelper.accessor('at', {
          header: 'Time',
          enableSorting: false,
          meta: { width: 120 },
          cell: ({ getValue }) => <span className="font-mono text-code-small">{formatEventTime(getValue())}</span>,
        }),
        resultHelper.accessor('status', {
          header: 'Status',
          enableSorting: false,
          meta: { width: 120 },
          cell: ({ getValue }) => <UptimeStatusBadge status={getValue()} />,
        }),
        resultHelper.display({
          id: 'detail',
          header: 'Detail',
          cell: ({ row }) =>
            row.original.error ? (
              <span className="block truncate text-text-secondary">{row.original.error}</span>
            ) : (
              <span className="block truncate text-text-tertiary">{describeTimings(row.original)}</span>
            ),
        }),
        resultHelper.accessor('code', {
          header: 'Code',
          enableSorting: false,
          meta: { width: 80 },
          cell: ({ getValue }) => <span className="font-mono text-code-small">{getValue() ?? '—'}</span>,
        }),
        resultHelper.accessor('response_ms', {
          header: 'Response',
          enableSorting: false,
          meta: { width: 100, align: 'right' },
          cell: ({ getValue }) => formatMs(getValue()),
        }),
      ]),
    []
  );
  const rows = useMemo(() => results.map((result, index) => ({ ...result, id: `${result.at}-${index}` })), [results]);
  const table = useDataTable({ columns, data: rows });

  return <DataTable table={table} label="Recent Checks" variant="card" size="sm" />;
}
