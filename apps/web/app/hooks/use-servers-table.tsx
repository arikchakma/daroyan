import type { Schemas } from '@daroyan/client';
import { createColumnHelper } from '@tanstack/react-table';
import type { SortingState } from '@tanstack/react-table';
import { useMemo } from 'react';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { ServerStatusBadge } from '~/components/servers/server-status-badge';
import { UsageBar } from '~/components/servers/usage-bar';
import { Menu } from '~/components/ui/menu';
import { SortableHeader } from '~/components/ui/sortable-header';
import { useDataTable } from '~/hooks/use-data-table';
import { resolveUpdater } from '~/lib/table';
import type { DataTableFeatures } from '~/lib/table';
import { formatBytes, formatRelative, formatUptimeDays } from '~/utils/format';

type ServerItem = Schemas['ServerItem'];

const helper = createColumnHelper<DataTableFeatures, ServerItem>();

function isLive(server: ServerItem) {
  return server.status === 'healthy' || server.status === 'warning';
}

function getSubtitle(server: ServerItem) {
  return [
    server.private_ip,
    [server.platform, server.platform_version].filter(Boolean).join(' '),
    server.cpu_cores && server.mem_total ? `${server.cpu_cores} vCPU, ${formatBytes(server.mem_total)}` : null,
  ]
    .filter(Boolean)
    .join(' · ');
}

type UseServersTableOptions = {
  servers: ServerItem[];
  sorting: SortingState;
  onSortingChange: (sorting: SortingState) => void;
};

export function useServersTable(options: UseServersTableOptions) {
  const { servers, sorting, onSortingChange } = options;

  const columns = useMemo(
    () =>
      helper.columns([
        helper.accessor('name', {
          id: 'name',
          header: ({ column }) => <SortableHeader column={column} label="Server" />,
          sortFn: 'text',
          sortDescFirst: false,
          meta: { skeleton: 'lines' },
          cell: ({ row }) => (
            <span className="flex min-w-0 flex-col gap-0.5">
              <Link
                to={`/servers/${row.original.id}`}
                className="truncate font-medium outline-none after:absolute after:inset-0 focus-visible:underline"
              >
                {row.original.name}
              </Link>
              <span className="truncate text-small text-text-tertiary">
                {getSubtitle(row.original) || 'Waiting for the first report'}
              </span>
            </span>
          ),
        }),
        helper.accessor('status', {
          header: 'Status',
          enableSorting: false,
          meta: { width: 120, skeleton: 'badge' },
          cell: ({ getValue }) => <ServerStatusBadge status={getValue()} />,
        }),
        helper.accessor((server) => (isLive(server) ? (server.cpu ?? 0) : -1), {
          id: 'cpu',
          header: ({ column }) => <SortableHeader column={column} label="CPU" />,
          sortFn: 'basic',
          sortDescFirst: true,
          meta: { width: 120, hideBelow: 'lg' },
          cell: ({ row }) => <UsageBar percent={isLive(row.original) ? row.original.cpu : null} />,
        }),
        helper.accessor((server) => (isLive(server) ? (server.mem_percent ?? 0) : -1), {
          id: 'memory',
          header: ({ column }) => <SortableHeader column={column} label="Memory" />,
          sortFn: 'basic',
          sortDescFirst: true,
          meta: { width: 120, hideBelow: 'lg' },
          cell: ({ row }) => <UsageBar percent={isLive(row.original) ? row.original.mem_percent : null} />,
        }),
        helper.accessor((server) => (isLive(server) ? (server.disk_percent ?? 0) : -1), {
          id: 'disk',
          header: ({ column }) => <SortableHeader column={column} label="Disk" />,
          sortFn: 'basic',
          sortDescFirst: true,
          meta: { width: 120, hideBelow: 'lg' },
          cell: ({ row }) => <UsageBar percent={isLive(row.original) ? row.original.disk_percent : null} />,
        }),
        helper.accessor('uptime', {
          header: 'Uptime',
          enableSorting: false,
          meta: { width: 90, align: 'right', hideBelow: 'md' },
          cell: ({ row }) =>
            isLive(row.original) && row.original.uptime !== null && row.original.uptime !== undefined ? (
              formatUptimeDays(row.original.uptime)
            ) : (
              <span className="text-text-tertiary">—</span>
            ),
        }),
        helper.accessor((server) => (server.last_seen_at ? Date.parse(server.last_seen_at) : 0), {
          id: 'lastSeen',
          header: ({ column }) => <SortableHeader column={column} label="Last Seen" />,
          sortFn: 'basic',
          sortDescFirst: true,
          meta: { width: 130, align: 'right' },
          cell: ({ row }) => (
            <span className="text-text-secondary">
              {row.original.last_seen_at ? formatRelative(row.original.last_seen_at) : 'Never'}
            </span>
          ),
        }),
        helper.display({
          id: 'actions',
          enableHiding: false,
          meta: { width: 36, align: 'right', skeleton: 'none' },
          header: () => <span className="sr-only">Actions</span>,
          cell: ({ row }) => (
            <Menu.Root>
              <Menu.Trigger
                aria-label={`Actions for ${row.original.name}`}
                className="relative z-1 inline-flex size-6 items-center justify-center rounded-full opacity-0 outline-none group-hover:opacity-100 hover:bg-surface-fill focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-text-link/40 data-popup-open:opacity-100 pointer-coarse:opacity-100"
              >
                <Icon name="more" />
              </Menu.Trigger>
              <Menu.Portal>
                <Menu.Positioner align="end" sideOffset={4}>
                  <Menu.Popup>
                    <Menu.LinkItem render={<Link to={`/servers/${row.original.id}`} />}>
                      <Icon name="eye" />
                      View Server
                    </Menu.LinkItem>
                  </Menu.Popup>
                </Menu.Positioner>
              </Menu.Portal>
            </Menu.Root>
          ),
        }),
      ]),
    []
  );

  return useDataTable({
    columns,
    data: servers,
    state: { sorting },
    onSortingChange: (updater) => onSortingChange(resolveUpdater(updater, sorting)),
  });
}
