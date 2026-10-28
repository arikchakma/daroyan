import type { Schemas } from '@daroyan/client';
import { createColumnHelper } from '@tanstack/react-table';
import { useMemo } from 'react';

import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';
import { Button } from '~/components/ui/button';
import { DataTable } from '~/components/ui/data-table';
import { formatLasted } from '~/utils/uptime';
import { useDataTable } from '~/hooks/use-data-table';
import type { DataTableFeatures } from '~/lib/table';
import { formatClock, formatDate } from '~/utils/format';

const outageHelper = createColumnHelper<DataTableFeatures, Schemas['UptimeOutage']>();

type OutagesTableProps = {
  outages: Schemas['UptimeOutage'][];
  onDeclare: (outage: Schemas['UptimeOutage']) => void;
};

export function OutagesTable(props: OutagesTableProps) {
  const { outages, onDeclare } = props;

  const columns = useMemo(
    () =>
      outageHelper.columns([
        outageHelper.accessor('started_at', {
          header: 'Started',
          enableSorting: false,
          meta: { width: 140 },
          cell: ({ getValue }) => (
            <span className="font-mono text-code-small">
              {formatDate(getValue())}, {formatClock(Date.parse(getValue()))}
            </span>
          ),
        }),
        outageHelper.accessor('cause', {
          header: 'Cause',
          enableSorting: false,
          cell: ({ getValue }) => <span className="block truncate font-medium">{getValue()}</span>,
        }),
        outageHelper.display({
          id: 'duration',
          header: 'Duration',
          meta: { width: 100, align: 'right' },
          cell: ({ row }) => formatLasted(row.original),
        }),
        outageHelper.accessor('resolved_at', {
          header: 'Status',
          enableSorting: false,
          meta: { width: 120 },
          cell: ({ getValue, row }) =>
            getValue() ? (
              <Badge tone="success">
                <Icon name="check" />
                Resolved
              </Badge>
            ) : row.original.kind === 'degraded' ? (
              <Badge tone="warning">
                <Icon name="triangle-alert" />
                Slow
              </Badge>
            ) : (
              <Badge tone="critical">
                <Icon name="circle-x" />
                Down
              </Badge>
            ),
        }),
        outageHelper.display({
          id: 'declare',
          header: '',
          meta: { width: 150, align: 'right' },
          cell: ({ row }) =>
            !row.original.resolved_at && (
              <Button variant="secondary" size="xs" className="ml-auto" onClick={() => onDeclare(row.original)}>
                <Icon name="flag" />
                Declare Incident
              </Button>
            ),
        }),
      ]),
    [onDeclare]
  );
  const table = useDataTable({ columns, data: outages });

  return <DataTable table={table} label="Outages" variant="card" size="sm" />;
}
