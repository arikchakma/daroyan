import type { Schemas } from '@daroyan/client';
import { createColumnHelper } from '@tanstack/react-table';
import { useMemo } from 'react';

import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';
import { useDataTable } from '~/hooks/use-data-table';
import type { DataTableFeatures } from '~/lib/table';
import { formatPercent, formatSpeed, formatUsedOf, getPercent } from '~/utils/format';

type DiskRow = Schemas['DiskState'] & { id: string };

const helper = createColumnHelper<DataTableFeatures, DiskRow>();

function Muted() {
  return <span className="text-text-tertiary">—</span>;
}

export function useFilesystemsTable(disks: Schemas['DiskState'][]) {
  const rows = useMemo(() => disks.map((disk) => ({ ...disk, id: disk.mount })), [disks]);

  const columns = useMemo(
    () =>
      helper.columns([
        helper.accessor('mount', {
          header: 'Filesystem',
          enableSorting: false,
          cell: ({ getValue }) => <span className="block truncate font-mono font-medium">{getValue()}</span>,
        }),
        helper.display({
          id: 'status',
          header: 'Status',
          meta: { width: 110, skeleton: 'badge' },
          cell: ({ row }) =>
            getPercent(row.original.disk_used, row.original.disk_total) >= 90 ? (
              <Badge tone="critical">
                <Icon name="triangle-alert" />
                Full
              </Badge>
            ) : (
              <Badge tone="success">
                <Icon name="check" />
                OK
              </Badge>
            ),
        }),
        helper.display({
          id: 'space',
          header: 'Space',
          meta: { width: 160, align: 'right' },
          cell: ({ row }) =>
            `${formatPercent(getPercent(row.original.disk_used, row.original.disk_total))} · ${formatUsedOf(row.original.disk_used, row.original.disk_total)}`,
        }),
        helper.display({
          id: 'inodes',
          header: 'Inodes',
          meta: { width: 72, align: 'right', hideBelow: 'md' },
          cell: ({ row }) =>
            row.original.inode_total > 0 ? formatPercent(getPercent(row.original.inode_used, row.original.inode_total)) : <Muted />,
        }),
        helper.accessor('read_speed', {
          header: 'Read',
          enableSorting: false,
          meta: { width: 84, align: 'right', hideBelow: 'md' },
          cell: ({ getValue }) => (getValue() > 0 ? formatSpeed(getValue()) : <Muted />),
        }),
        helper.accessor('write_speed', {
          header: 'Write',
          enableSorting: false,
          meta: { width: 84, align: 'right', hideBelow: 'md' },
          cell: ({ getValue }) => (getValue() > 0 ? formatSpeed(getValue()) : <Muted />),
        }),
      ]),
    []
  );

  return useDataTable({ columns, data: rows });
}
