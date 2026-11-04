import { useMutation, useQuery } from '@tanstack/react-query';
import type { RowSelectionState } from '@tanstack/react-table';
import { useState } from 'react';
import { Link } from 'react-router';

import type { RuleState } from '~/components/alerts/rule-status-badge';
import { Icon } from '~/components/icons/icon';
import { BulkAction } from '~/components/ui/bulk-action';
import { BulkBar } from '~/components/ui/bulk-bar';
import { Button } from '~/components/ui/button';
import { ConfirmDialog } from '~/components/ui/confirm-dialog';
import { DataTable } from '~/components/ui/data-table';
import { EmptyState, NoResults } from '~/components/ui/empty-state';
import { PageBar } from '~/components/ui/page-bar';
import { ViewsToolbar } from '~/components/ui/views-toolbar';
import { useAlertRulesTable } from '~/hooks/use-alert-rules-table';
import type { RuleRow } from '~/hooks/use-alert-rules-table';
import { useSortParams } from '~/hooks/use-sort-params';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { useViewParams } from '~/hooks/use-list-params';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import {
  alertMuteQueries,
  alertQueries,
  alertRuleQueries,
  deleteAlertRuleOptions,
  integrationQueries,
  updateAlertRuleOptions,
} from '~/queries/alerts';
import { serverQueries } from '~/queries/servers';
import {
  describeMuteEnd,
  describeScope,
  getAlertMute,
  getRuleState,
  isCheckKind,
  isCronKind,
  isWholeRuleMute,
} from '~/utils/alerts';
import { formatClock, pluralize } from '~/utils/format';

type RuleView = 'all' | RuleState;

const VIEWS: { value: RuleView; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'firing', label: 'Firing' },
  { value: 'muted', label: 'Muted' },
  { value: 'limited', label: 'Limit Reached' },
  { value: 'ok', label: 'OK' },
  { value: 'disabled', label: 'Disabled' },
];

const VIEW_VALUES = VIEWS.map((view) => view.value);

export function meta() {
  return [{ title: 'Alerts · Daroyan' }];
}

export default function AlertsPage() {
  const isAdmin = useIsAdmin();
  const { data: rules, isLoading } = useQuery(alertRuleQueries.list());
  const { data: firing } = useQuery(alertQueries.list('firing'));
  const { data: mutes } = useQuery(alertMuteQueries.list());
  const { data: integrations } = useQuery(integrationQueries.list());
  const { data: servers } = useQuery(serverQueries.list());
  const { search, view, setView, setSearch, reset, isFiltered } = useViewParams(VIEW_VALUES);
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({});
  const [isDeleteOpen, setIsDeleteOpen] = useState(false);

  const { sorting, onSortingChange } = useSortParams({
    sorts: { name: 'asc', status: 'asc', lastFired: 'desc' },
    defaultSort: 'status',
  });

  const rows: RuleRow[] = (rules ?? []).map((rule) => {
    const state = getRuleState(rule, firing ?? [], mutes ?? []);
    const ruleMutes = (mutes ?? []).filter((mute) => mute.rule_id === rule.id);
    const mutedNames = ruleMutes.map((mute) =>
      isWholeRuleMute(mute)
        ? `muted ${describeMuteEnd(mute)}`
        : `${mute.subject ?? firing?.find((alert) => getAlertMute([mute], alert))?.target_name ?? 'a target'} muted ${describeMuteEnd(mute)}`
    );
    const notes = [
      ...mutedNames,
      ...(state === 'limited' && rule.quiet_until
        ? [`quiet until ${formatClock(new Date(rule.quiet_until).getTime())} after ${rule.options.message_limit} messages`]
        : []),
    ];
    return {
      ...rule,
      summary: [rule.summary, ...notes].join(' · '),
      state,
      scopeLabel: describeScope(
        rule.scope,
        servers ?? [],
        [],
        isCheckKind(rule.condition.kind),
        [],
        isCronKind(rule.condition.kind)
      ),
      integrations: rule.integration_ids.flatMap(
        (id) => integrations?.find((integration) => integration.id === id) ?? []
      ),
    };
  });
  const term = search.trim().toLowerCase();
  const visible = rows.filter(
    (rule) =>
      (view === 'all' || rule.state === view) &&
      (!term || rule.name.toLowerCase().includes(term) || rule.summary.toLowerCase().includes(term))
  );
  const selectedIds = Object.keys(rowSelection).filter((id) => visible.some((rule) => rule.id === id));

  const table = useAlertRulesTable({
    rules: visible,
    sorting,
    onSortingChange,
    rowSelection,
    onRowSelectionChange: setRowSelection,
    canSelect: isAdmin,
  });

  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: alertRuleQueries.all() });
    queryClient.invalidateQueries({ queryKey: alertQueries.all() });
    setRowSelection({});
  };
  const { mutateAsync: updateRule } = useMutation({
    ...updateAlertRuleOptions(),
    meta: { errorMessage: 'The rules could not be updated' },
  });
  const { mutateAsync: deleteRule, isPending: isDeleting } = useMutation({
    ...deleteAlertRuleOptions(),
    meta: { errorMessage: 'The rules could not be deleted' },
  });

  const setEnabled = async (enabled: boolean) => {
    await Promise.all(selectedIds.map((ruleId) => updateRule({ ruleId, body: { enabled } })));
    toast.success(`${enabled ? 'Enabled' : 'Disabled'} ${pluralize(selectedIds.length, 'rule')}`);
    refresh();
  };
  const removeSelected = async () => {
    await Promise.all(selectedIds.map((ruleId) => deleteRule({ ruleId })));
    toast.success(`Deleted ${pluralize(selectedIds.length, 'rule')}`);
    setIsDeleteOpen(false);
    refresh();
  };

  const isEmpty = rules?.length === 0;

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar icon="bell" title="Alerts">
        {!isEmpty && isAdmin && (
          <Button nativeButton={false} render={<Link to="/alerts/new" />} className="w-fit">
            <Icon name="plus" />
            New Rule
          </Button>
        )}
      </PageBar>

      {isEmpty ? (
        <EmptyState
          icon="bell"
          marks={['check', 'triangle-alert', 'check']}
          title="Create your first alert rule"
          description="Get a message in Slack, Discord or Telegram when a server crosses a limit, a check goes down or a cron fails."
        >
          {isAdmin && (
            <>
              <Button nativeButton={false} variant="secondary" render={<Link to="/settings/integrations" />} className="w-fit">
                Add Integration
              </Button>
              <Button nativeButton={false} render={<Link to="/alerts/new" />} className="w-fit">
                <Icon name="plus" />
                New Rule
              </Button>
            </>
          )}
        </EmptyState>
      ) : (
        <div className="flex flex-col gap-2 px-3 pt-1 pb-3">
          <ViewsToolbar
            views={VIEWS}
            view={view}
            onViewChange={setView}
            searchLabel="Search by name or condition"
            search={search}
            onSearchChange={setSearch}
            isViewChanged={isFiltered}
            onReset={reset}
          />
          <DataTable
            table={table}
            label="Alert Rules"
            size="lg"
            isLoading={isLoading}
            bulkBar={
              isAdmin && (
                <BulkBar
                  label="Rules"
                  countLabel={selectedIds.length.toString()}
                  pageCount={visible.length}
                  onSelectPage={() => table.toggleAllPageRowsSelected(true)}
                  onClear={() => setRowSelection({})}
                >
                  <BulkAction onClick={() => void setEnabled(true)}>Enable</BulkAction>
                  <BulkAction onClick={() => void setEnabled(false)}>Disable</BulkAction>
                  <BulkAction onClick={() => setIsDeleteOpen(true)} className="text-tone-critical-fg">
                    Delete
                  </BulkAction>
                </BulkBar>
              )
            }
          />
          {rules && visible.length === 0 && view === 'firing' && !search ? (
            <div className="flex flex-col items-center gap-1 py-12 text-center">
              <Icon name="circle-check" className="mb-2 size-5 text-tone-success-fg" />
              <p className="text-body font-medium text-text-primary">Nothing is firing</p>
              <p className="text-body text-text-secondary">
                {rules.length === 1 ? 'Your rule is quiet.' : `All ${rules.length} rules are quiet.`} A rule shows
                up here while it fires.
              </p>
              <Button variant="secondary" className="mt-3 w-fit" onClick={() => setView('all')}>
                View All Rules
              </Button>
            </div>
          ) : (
            rules &&
            visible.length === 0 && (
              <NoResults title="No rules found" onReset={reset} />
            )
          )}
        </div>
      )}

      <ConfirmDialog
        open={isDeleteOpen}
        onOpenChange={setIsDeleteOpen}
        title={`Delete ${pluralize(selectedIds.length, 'rule')}?`}
        description="Their alert history and notification log are deleted too. Firing alerts stop without a resolve message."
        confirmLabel="Delete"
        isPending={isDeleting}
        onConfirm={() => void removeSelected()}
      />
    </div>
  );
}
