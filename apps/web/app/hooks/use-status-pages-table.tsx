import type { Schemas } from '@daroyan/client';
import { createColumnHelper } from '@tanstack/react-table';
import type { SortingState } from '@tanstack/react-table';
import { useMemo } from 'react';
import { Link } from 'react-router';

import { PageStatusBadge, VisibilityBadge } from '~/components/status-pages/status-page-badges';
import { SortableHeader } from '~/components/ui/sortable-header';
import { useDataTable } from '~/hooks/use-data-table';
import { resolveUpdater } from '~/lib/table';
import type { DataTableFeatures } from '~/lib/table';
import { formatRelative } from '~/utils/format';
import { getPublicUrl } from '~/utils/status-pages';

const helper = createColumnHelper<DataTableFeatures, Schemas['StatusPage']>();

type UseStatusPagesTableOptions = {
  pages: Schemas['StatusPage'][];
  sorting: SortingState;
  onSortingChange: (sorting: SortingState) => void;
};

export function useStatusPagesTable(options: UseStatusPagesTableOptions) {
  const { pages, sorting, onSortingChange } = options;

  const columns = useMemo(
    () =>
      helper.columns([
        helper.accessor('name', {
          id: 'name',
          header: ({ column }) => <SortableHeader column={column} label="Status Page" />,
          sortFn: 'text',
          sortDescFirst: false,
          meta: { skeleton: 'lines' },
          cell: ({ row }) => (
            <span className="flex min-w-0 flex-col gap-0.5">
              <Link
                to={`/status-pages/${row.original.id}`}
                className="truncate outline-none after:absolute after:inset-0 focus-visible:underline"
              >
                {row.original.name}
              </Link>
              <span className="truncate font-mono text-code-small text-text-tertiary">
                {getPublicUrl(row.original.slug).replace(/^https?:\/\//, '')}
              </span>
            </span>
          ),
        }),
        helper.display({
          id: 'status',
          header: 'Status',
          meta: { width: 150, skeleton: 'badge' },
          cell: ({ row }) => <PageStatusBadge status={row.original.status} />,
        }),
        helper.accessor('component_count', {
          id: 'components',
          header: ({ column }) => <SortableHeader column={column} label="Components" />,
          sortFn: 'basic',
          sortDescFirst: true,
          meta: { width: 130, hideBelow: 'md' },
        }),
        helper.display({
          id: 'visibility',
          header: 'Visibility',
          meta: { width: 120, align: 'right', skeleton: 'badge' },
          cell: ({ row }) => <VisibilityBadge isPublic={row.original.is_public} />,
        }),
        helper.accessor((page) => new Date(page.updated_at).getTime(), {
          id: 'updated',
          header: ({ column }) => <SortableHeader column={column} label="Updated" />,
          sortFn: 'basic',
          sortDescFirst: true,
          meta: { width: 130, align: 'right', hideBelow: 'md' },
          cell: ({ row }) => <span className="text-text-secondary">{formatRelative(row.original.updated_at)}</span>,
        }),
      ]),
    []
  );

  return useDataTable({
    columns,
    data: pages,
    state: { sorting },
    onSortingChange: (updater) => onSortingChange(resolveUpdater(updater, sorting)),
  });
}
