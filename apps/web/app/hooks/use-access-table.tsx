import { createColumnHelper } from '@tanstack/react-table';
import { useMemo } from 'react';

import { Badge } from '~/components/ui/badge';
import { Dash } from '~/components/ui/dash';
import { useDataTable } from '~/hooks/use-data-table';
import type { DataTableFeatures } from '~/lib/table';
import type { AccessEvent } from '~/utils/access';
import { getAccessDetail, getAccessLabel, getAccessTone, getAccessUser, isNewAddress } from '~/utils/access';
import { formatEventTime } from '~/utils/format';

type AccessRow = AccessEvent & { id: string };

const helper = createColumnHelper<DataTableFeatures, AccessRow>();

export function useAccessTable(events: AccessEvent[]) {
  const rows = useMemo(() => events.map((event, index) => ({ ...event, id: `${event.at}-${index}` })), [events]);

  const columns = useMemo(
    () =>
      helper.columns([
        helper.accessor('at', {
          header: 'Time',
          enableSorting: false,
          meta: { width: 110 },
          cell: ({ getValue }) => <span className="tabular-nums">{formatEventTime(getValue())}</span>,
        }),
        helper.accessor('action', {
          header: 'Event',
          enableSorting: false,
          meta: { width: 150, skeleton: 'badge' },
          cell: ({ row }) => <Badge tone={getAccessTone(row.original)}>{getAccessLabel(row.original)}</Badge>,
        }),
        helper.display({
          id: 'user',
          header: 'User',
          meta: { width: 170 },
          cell: ({ row }) => {
            const user = getAccessUser(row.original);
            return user ? <span className="block truncate font-medium">{user}</span> : <Dash />;
          },
        }),
        helper.accessor('ip', {
          header: 'From',
          enableSorting: false,
          meta: { width: 200 },
          cell: ({ row, getValue }) => {
            const ip = getValue();
            if (!ip) {
              return <Dash />;
            }

            return (
              <span className="flex min-w-0 items-center gap-2">
                <span className="truncate font-mono">{ip}</span>
                {isNewAddress(row.original) && <Badge tone="info">New</Badge>}
              </span>
            );
          },
        }),
        helper.display({
          id: 'detail',
          header: 'Detail',
          cell: ({ row }) => {
            const detail = getAccessDetail(row.original);
            return detail ? (
              <span className="block truncate font-mono text-text-secondary">{detail}</span>
            ) : (
              <Dash />
            );
          },
        }),
      ]),
    []
  );

  return useDataTable({ columns, data: rows });
}
