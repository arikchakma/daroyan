import type { Schemas } from '@daroyan/client';
import { createColumnHelper } from '@tanstack/react-table';
import { useMemo } from 'react';

import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';
import { Button } from '~/components/ui/button';
import { DataTable } from '~/components/ui/data-table';
import { useDataTable } from '~/hooks/use-data-table';
import type { DataTableFeatures } from '~/lib/table';
import { formatClock, formatDate, formatDuration } from '~/utils/format';

const failureHelper = createColumnHelper<DataTableFeatures, Schemas['CronFailure']>();

function formatLasted(failure: Schemas['CronFailure']) {
  const end = failure.resolved_at ? Date.parse(failure.resolved_at) : Date.now();
  const seconds = Math.round((end - Date.parse(failure.started_at)) / 1000);
  return seconds < 60 ? `${Math.max(seconds, 0)}s` : formatDuration(seconds);
}

type FailuresTableProps = {
  failures: Schemas['CronFailure'][];
  onDeclare: (failure: Schemas['CronFailure']) => void;
};

export function FailuresTable(props: FailuresTableProps) {
  const { failures, onDeclare } = props;

  const columns = useMemo(
    () =>
      failureHelper.columns([
        failureHelper.accessor('started_at', {
          header: 'Started',
          enableSorting: false,
          meta: { width: 140 },
          cell: ({ getValue }) => (
            <span className="font-mono text-code-small">
              {formatDate(getValue())}, {formatClock(Date.parse(getValue()))}
            </span>
          ),
        }),
        failureHelper.display({
          id: 'cause',
          header: 'Cause',
          cell: ({ row }) => (
            <span className="flex min-w-0 flex-col gap-0.5">
              <span className="truncate font-medium">{row.original.kind === 'missed' ? 'Missed run' : 'Failed run'}</span>
              <span className="truncate font-mono text-code-small text-text-tertiary">{row.original.cause}</span>
            </span>
          ),
        }),
        failureHelper.display({
          id: 'duration',
          header: 'Duration',
          meta: { width: 100, align: 'right' },
          cell: ({ row }) => formatLasted(row.original),
        }),
        failureHelper.accessor('resolved_at', {
          header: 'Status',
          enableSorting: false,
          meta: { width: 120 },
          cell: ({ getValue }) =>
            getValue() ? (
              <Badge tone="success">
                <Icon name="check" />
                Resolved
              </Badge>
            ) : (
              <Badge tone="critical">
                <Icon name="circle-x" />
                Ongoing
              </Badge>
            ),
        }),
        failureHelper.display({
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
  const table = useDataTable({ columns, data: failures });

  return <DataTable table={table} label="Failures" variant="card" size="lg" />;
}
