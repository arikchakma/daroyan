import type { Schemas } from '@daroyan/client';
import { createColumnHelper } from '@tanstack/react-table';
import { useMemo } from 'react';

import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';
import { useDataTable } from '~/hooks/use-data-table';
import type { DataTableFeatures } from '~/lib/table';

type PortRow = Schemas['PortState'] & { id: string };

const helper = createColumnHelper<DataTableFeatures, PortRow>();

export function usePortsTable(ports: Schemas['PortState'][]) {
  const rows = useMemo(() => ports.map((port) => ({ ...port, id: `${port.host}:${port.port}` })), [ports]);

  const columns = useMemo(
    () =>
      helper.columns([
        helper.accessor('host', {
          header: 'Host',
          enableSorting: false,
          cell: ({ row, getValue }) =>
            row.original.name ? (
              <span className="flex min-w-0 items-center gap-2">
                <span className="truncate font-medium">{row.original.name}</span>
                <span className="truncate font-mono text-text-secondary">{getValue()}</span>
              </span>
            ) : (
              <span className="block truncate font-mono font-medium">{getValue()}</span>
            ),
        }),
        helper.accessor('open', {
          header: 'Status',
          enableSorting: false,
          meta: { width: 110, skeleton: 'badge' },
          cell: ({ getValue }) =>
            getValue() ? (
              <Badge tone="success">
                <Icon name="check" />
                Open
              </Badge>
            ) : (
              <Badge tone="critical">
                <Icon name="circle-x" />
                Closed
              </Badge>
            ),
        }),
        helper.accessor('port', {
          header: 'Protocol',
          enableSorting: false,
          meta: { width: 170 },
          cell: ({ getValue }) => `TCP on ${getValue()}`,
        }),
        helper.accessor('latency_ms', {
          header: 'Response',
          enableSorting: false,
          meta: { width: 90, align: 'right' },
          cell: ({ getValue }) => {
            const latency = getValue();
            return latency === null || latency === undefined ? (
              <span className="text-text-tertiary">—</span>
            ) : (
              `${latency} ms`
            );
          },
        }),
      ]),
    []
  );

  return useDataTable({ columns, data: rows });
}
