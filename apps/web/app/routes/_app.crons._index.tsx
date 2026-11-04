import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { DataTable } from '~/components/ui/data-table';
import { EmptyState, NoResults } from '~/components/ui/empty-state';
import { PageBar } from '~/components/ui/page-bar';
import { ViewsToolbar } from '~/components/ui/views-toolbar';
import { useCronsTable } from '~/hooks/use-crons-table';
import { useFiringAlerts } from '~/hooks/use-firing-alerts';
import type { CronRow } from '~/hooks/use-crons-table';
import { useSortParams } from '~/hooks/use-sort-params';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { useViewParams } from '~/hooks/use-list-params';
import { cronQueries } from '~/queries/crons';
import { describeMuteEnd, getAlertsForTarget, getSharedMute } from '~/utils/alerts';

type StatusView = 'all' | 'ok' | 'late' | 'failed' | 'paused';

const VIEWS: { value: StatusView; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'ok', label: 'OK' },
  { value: 'late', label: 'Late' },
  { value: 'failed', label: 'Failed' },
  { value: 'paused', label: 'Paused' },
];

const VIEW_VALUES = VIEWS.map((view) => view.value);

export function meta() {
  return [{ title: 'Crons · Daroyan' }];
}

export default function CronsPage() {
  const isAdmin = useIsAdmin();
  const { data: crons, isLoading } = useQuery(cronQueries.list());
  const { alerts: firing, mutes } = useFiringAlerts();
  const { search, view, setView, setSearch, reset, isFiltered } = useViewParams(VIEW_VALUES);

  const { sorting, onSortingChange } = useSortParams({
    sorts: { name: 'asc', duration: 'desc', lastPing: 'desc', next: 'asc' },
    defaultSort: 'name',
  });

  const term = search.trim().toLowerCase();
  const rows: CronRow[] = (crons ?? []).map((cron) => {
    const mute = getSharedMute(getAlertsForTarget(firing, { cronId: cron.id }), mutes);
    return mute ? { ...cron, mutedUntil: describeMuteEnd(mute) } : cron;
  });
  const visible = rows.filter(
    (cron) =>
      (view === 'all' || cron.status === view) &&
      (!term || cron.name.toLowerCase().includes(term) || (cron.command ?? '').toLowerCase().includes(term))
  );

  const table = useCronsTable({ crons: visible, sorting, onSortingChange });
  const isEmpty = crons?.length === 0;

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar icon="clock" title="Crons">
        {!isEmpty && isAdmin && (
          <Button nativeButton={false} render={<Link to="/crons/new" />} className="w-fit">
            <Icon name="plus" />
            New Cron
          </Button>
        )}
      </PageBar>

      {isEmpty ? (
        <EmptyState
          icon="clock"
          marks={['check', 'circle-x', 'clock']}
          title="Watch your first cron"
          description="Your job pings Daroyan each time it runs. You hear about it when a ping is late, missing or reports a failure."
        >
          {isAdmin && (
            <Button nativeButton={false} render={<Link to="/crons/new" />} className="w-fit">
              <Icon name="plus" />
              New Cron
            </Button>
          )}
        </EmptyState>
      ) : (
        <div className="flex flex-col gap-2 px-3 pt-1 pb-3">
          <ViewsToolbar
            views={VIEWS}
            view={view}
            onViewChange={setView}
            searchLabel="Search by name or command"
            search={search}
            onSearchChange={setSearch}
            isViewChanged={isFiltered}
            onReset={reset}
          />
          <DataTable table={table} label="Crons" size="lg" isLoading={isLoading} />
          {crons && visible.length === 0 && (
            <NoResults title="No crons found" onReset={reset} />
          )}
        </div>
      )}
    </div>
  );
}
