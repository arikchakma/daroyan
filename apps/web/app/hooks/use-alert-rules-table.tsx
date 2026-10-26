import type { Schemas } from '@daroyan/client';
import { createColumnHelper } from '@tanstack/react-table';
import type { RowSelectionState, SortingState } from '@tanstack/react-table';
import { useMemo } from 'react';
import { Link } from 'react-router';

import { RuleStatusBadge } from '~/components/alerts/rule-status-badge';
import type { RuleState } from '~/components/alerts/rule-status-badge';
import { Icon } from '~/components/icons/icon';
import { SelectAllCheckbox } from '~/components/ui/select-all-checkbox';
import { SelectRowCheckbox } from '~/components/ui/select-row-checkbox';
import { SortableHeader } from '~/components/ui/sortable-header';
import { useDataTable } from '~/hooks/use-data-table';
import { resolveUpdater } from '~/lib/table';
import type { DataTableFeatures } from '~/lib/table';
import { INTEGRATION_KINDS, isIntegrationKind } from '~/utils/alerts';
import { formatRelative } from '~/utils/format';

export type RuleRow = Schemas['AlertRule'] & {
  state: RuleState;
  scopeLabel: string;
  integrations: Pick<Schemas['Integration'], 'id' | 'kind' | 'name'>[];
};

const helper = createColumnHelper<DataTableFeatures, RuleRow>();

const STATE_ORDER: Record<RuleState, number> = { firing: 0, limited: 1, muted: 2, ok: 3, disabled: 4 };

type UseAlertRulesTableOptions = {
  rules: RuleRow[];
  sorting: SortingState;
  onSortingChange: (sorting: SortingState) => void;
  rowSelection: RowSelectionState;
  onRowSelectionChange: (selection: RowSelectionState) => void;
  canSelect: boolean;
};

export function useAlertRulesTable(options: UseAlertRulesTableOptions) {
  const { rules, sorting, onSortingChange, rowSelection, onRowSelectionChange, canSelect } = options;

  const columns = useMemo(
    () =>
      helper.columns([
        helper.display({
          id: 'select',
          header: ({ table }) => <SelectAllCheckbox table={table} label="Rules" />,
          meta: { width: 40, skeleton: 'checkbox' },
          cell: ({ row }) => <SelectRowCheckbox row={row} label={row.original.name} />,
        }),
        helper.accessor('name', {
          id: 'name',
          header: ({ column }) => <SortableHeader column={column} label="Rule" />,
          sortFn: 'text',
          sortDescFirst: false,
          meta: { skeleton: 'lines' },
          cell: ({ row }) => (
            <span className="flex min-w-0 flex-col gap-0.5">
              <Link
                to={`/alerts/${row.original.id}`}
                className="truncate font-medium outline-none after:absolute after:inset-0 focus-visible:underline"
              >
                {row.original.name}
              </Link>
              <span className="truncate text-small text-text-tertiary">{row.original.summary}</span>
            </span>
          ),
        }),
        helper.accessor('scopeLabel', {
          header: 'Servers',
          enableSorting: false,
          meta: { width: 150, hideBelow: 'md' },
          cell: ({ row }) => {
            const { scope, scopeLabel } = row.original;
            if (scope.kind === 'all') {
              return scopeLabel;
            }
            return (
              <span className="flex min-w-0 items-center gap-1.5">
                <Icon name={scope.kind === 'label' ? 'tag' : 'server'} className="size-3.5 shrink-0 text-icon-secondary" />
                <span className="truncate">{scope.kind === 'label' ? scope.label : scopeLabel}</span>
              </span>
            );
          },
        }),
        helper.accessor('integrations', {
          header: 'Notify',
          enableSorting: false,
          meta: { width: 240, hideBelow: 'lg' },
          cell: ({ getValue }) => {
            const integrations = getValue();
            if (integrations.length === 0) {
              return <span className="text-text-tertiary">Nobody</span>;
            }
            const shown = integrations.length > 3 ? integrations.slice(0, 2) : integrations;
            const hidden = integrations.length - shown.length;
            return (
              <span className="flex min-w-0 items-center gap-1.5 overflow-hidden">
                {shown.map((integration) => (
                  <span
                    key={integration.id}
                    className="flex min-w-0 items-center gap-1.25 rounded-md bg-surface-fill py-0.75 pr-1.75 pl-1.5 text-small"
                  >
                    <Icon
                      name={isIntegrationKind(integration.kind) ? INTEGRATION_KINDS[integration.kind].icon : 'webhook'}
                      className="size-3 shrink-0"
                    />
                    <span className="truncate">{integration.name}</span>
                  </span>
                ))}
                {hidden > 0 && (
                  <span
                    title={integrations.slice(2).map((integration) => integration.name).join(', ')}
                    className="shrink-0 rounded-md bg-surface-fill px-1.75 py-0.75 text-small text-text-secondary"
                  >
                    +{hidden}
                  </span>
                )}
              </span>
            );
          },
        }),
        helper.accessor((rule) => STATE_ORDER[rule.state], {
          id: 'status',
          header: ({ column }) => <SortableHeader column={column} label="Status" />,
          sortFn: 'basic',
          sortDescFirst: false,
          meta: { width: 130, skeleton: 'badge' },
          cell: ({ row }) => <RuleStatusBadge state={row.original.state} />,
        }),
        helper.accessor((rule) => (rule.last_fired_at ? new Date(rule.last_fired_at).getTime() : 0), {
          id: 'lastFired',
          header: ({ column }) => <SortableHeader column={column} label="Last Triggered" />,
          sortFn: 'basic',
          sortDescFirst: true,
          meta: { width: 130, align: 'right', hideBelow: 'sm' },
          cell: ({ row }) =>
            row.original.last_fired_at ? (
              formatRelative(row.original.last_fired_at)
            ) : (
              <span className="text-text-tertiary">—</span>
            ),
        }),
      ]).filter((column) => canSelect || column.id !== 'select'),
    [canSelect]
  );

  return useDataTable({
    data: rules,
    columns,
    state: { sorting, rowSelection },
    onSortingChange: (updater) => onSortingChange(resolveUpdater(updater, sorting)),
    onRowSelectionChange: (updater) => onRowSelectionChange(resolveUpdater(updater, rowSelection)),
  });
}
