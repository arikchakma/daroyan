import type { Schemas } from '@daroyan/client';
import { createColumnHelper } from '@tanstack/react-table';
import { useMemo } from 'react';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';
import { Tooltip } from '~/components/ui/tooltip';
import { useDataTable } from '~/hooks/use-data-table';
import type { DataTableFeatures } from '~/lib/table';
import { formatAlertDuration, getAlertMute, getAlertTargetPath } from '~/utils/alerts';
import { formatDate, formatClock } from '~/utils/format';

const helper = createColumnHelper<DataTableFeatures, Schemas['Alert']>();

export function useAlertHistoryTable(
  alerts: Schemas['Alert'][],
  mutes: Schemas['AlertMute'][] = [],
  valueHint?: string
) {
  const columns = useMemo(
    () =>
      helper.columns([
        helper.accessor('target_name', {
          header: 'Target',
          enableSorting: false,
          cell: ({ row }) => (
            <Link
              to={getAlertTargetPath(row.original)}
              className="block truncate outline-none hover:underline focus-visible:underline"
            >
              {row.original.target_name}
            </Link>
          ),
        }),
        helper.accessor('subject', {
          header: 'Subject',
          enableSorting: false,
          meta: { width: 150, hideBelow: 'md' },
          cell: ({ getValue }) => <span className="block truncate">{getValue()}</span>,
        }),
        helper.accessor('started_at', {
          header: 'Started',
          enableSorting: false,
          meta: { width: 130 },
          cell: ({ getValue }) => {
            const at = new Date(getValue()).getTime();
            return `${formatDate(at)}, ${formatClock(at)}`;
          },
        }),
        helper.accessor('resolved_at', {
          id: 'duration',
          header: 'Duration',
          enableSorting: false,
          meta: { width: 80, align: 'right' },
          cell: ({ row }) => formatAlertDuration(row.original.started_at, row.original.resolved_at),
        }),
        helper.accessor('value', {
          header: () => <ValueHeader hint={valueHint} />,
          enableSorting: false,
          meta: { width: 110, align: 'right' },
        }),
        helper.accessor('status', {
          header: 'Status',
          enableSorting: false,
          meta: { width: 110, skeleton: 'badge' },
          cell: ({ getValue, row }) =>
            getValue() === 'firing' && getAlertMute(mutes, row.original) ? (
              <Badge tone="neutral">
                <Icon name="bell-off" className="size-3.5" />
                Muted
              </Badge>
            ) : getValue() === 'firing' ? (
              <Badge tone={row.original.severity === 'warning' ? 'warning' : 'critical'}>
                <Icon name="triangle-alert" className="size-3.5" />
                {row.original.severity === 'warning' ? 'Warning' : 'Critical'}
              </Badge>
            ) : (
              <Badge tone="success">
                <Icon name="check" className="size-3.5" />
                Resolved
              </Badge>
            ),
        }),
      ]),
    [mutes, valueHint]
  );

  return useDataTable({ columns, data: alerts });
}

type ValueHeaderProps = {
  hint?: string;
};

function ValueHeader(props: ValueHeaderProps) {
  const { hint } = props;

  if (!hint) {
    return 'Value';
  }

  return (
    <span className="inline-flex items-center gap-1">
      Value
      <Tooltip.Root>
        <Tooltip.Trigger
          delay={200}
          aria-label="What the value means"
          className="inline-flex items-center text-text-tertiary outline-none hover:text-text-primary focus-visible:text-text-primary"
        >
          <Icon name="info" className="size-3.5" />
        </Tooltip.Trigger>
        <Tooltip.Portal>
          <Tooltip.Positioner side="top">
            <Tooltip.Popup className="max-w-64 text-left">{hint}</Tooltip.Popup>
          </Tooltip.Positioner>
        </Tooltip.Portal>
      </Tooltip.Root>
    </span>
  );
}
