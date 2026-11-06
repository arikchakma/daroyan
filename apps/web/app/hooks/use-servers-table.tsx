import type { Schemas } from '@daroyan/client';
import { createColumnHelper } from '@tanstack/react-table';
import type { SortingState } from '@tanstack/react-table';
import { useMemo } from 'react';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { ServerStatusBadge } from '~/components/servers/server-status-badge';
import { StarButton } from '~/components/servers/star-button';
import { UsageBar } from '~/components/servers/usage-bar';
import { Dash } from '~/components/ui/dash';
import { Menu } from '~/components/ui/menu';
import { SortableHeader } from '~/components/ui/sortable-header';
import { useDataTable } from '~/hooks/use-data-table';
import { resolveUpdater } from '~/lib/table';
import type { DataTableFeatures } from '~/lib/table';
import { formatRelative } from '~/utils/format';
import { isServerLive } from '~/utils/servers';

type ServerItem = Schemas['ServerItem'];

const helper = createColumnHelper<DataTableFeatures, ServerItem>();

const STARRED_SORT = { id: 'starred', desc: true };

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
        helper.accessor((server) => (server.starred_at ? 1 : 0), {
          id: 'starred',
          sortFn: 'basic',
          meta: { width: 36, skeleton: 'none' },
          header: () => (
            <span className="flex w-6 justify-center text-border-strong">
              <Icon name="star-fill" className="size-3.5" />
              <span className="sr-only">Starred</span>
            </span>
          ),
          cell: ({ row }) => <StarButton server={row.original} />,
        }),
        helper.accessor('name', {
          id: 'name',
          header: ({ column }) => <SortableHeader column={column} label="Server" />,
          sortFn: 'text',
          sortDescFirst: false,
          cell: ({ row }) => (
            <Link
              to={`/servers/${row.original.id}`}
              className="block truncate font-medium outline-none after:absolute after:inset-0 focus-visible:underline"
            >
              {row.original.name}
            </Link>
          ),
        }),
        helper.accessor('status', {
          header: 'Status',
          enableSorting: false,
          meta: { width: 120, skeleton: 'badge' },
          cell: ({ getValue }) => <ServerStatusBadge status={getValue()} />,
        }),
        helper.accessor((server) => (isServerLive(server) ? (server.cpu ?? 0) : -1), {
          id: 'cpu',
          header: ({ column }) => <SortableHeader column={column} label="CPU" />,
          sortFn: 'basic',
          sortDescFirst: true,
          meta: { width: 150, hideBelow: 'lg' },
          cell: ({ row }) => <UsageBar percent={isServerLive(row.original) ? row.original.cpu : null} />,
        }),
        helper.accessor((server) => (isServerLive(server) ? (server.mem_percent ?? 0) : -1), {
          id: 'memory',
          header: ({ column }) => <SortableHeader column={column} label="Memory" />,
          sortFn: 'basic',
          sortDescFirst: true,
          meta: { width: 150, hideBelow: 'lg' },
          cell: ({ row }) => <UsageBar percent={isServerLive(row.original) ? row.original.mem_percent : null} />,
        }),
        helper.accessor((server) => (isServerLive(server) ? (server.disk_percent ?? 0) : -1), {
          id: 'disk',
          header: ({ column }) => <SortableHeader column={column} label="Disk" />,
          sortFn: 'basic',
          sortDescFirst: true,
          meta: { width: 150, hideBelow: 'lg' },
          cell: ({ row }) => <UsageBar percent={isServerLive(row.original) ? row.original.disk_percent : null} />,
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
    state: { sorting: [STARRED_SORT, ...sorting] },
    onSortingChange: (updater) =>
      onSortingChange(
        resolveUpdater(updater, [STARRED_SORT, ...sorting]).filter((sort) => sort.id !== STARRED_SORT.id)
      ),
  });
}
