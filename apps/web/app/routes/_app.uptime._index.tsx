import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { DataTable } from '~/components/ui/data-table';
import { EmptyState, NoResults } from '~/components/ui/empty-state';
import { PageBar } from '~/components/ui/page-bar';
import { ViewsToolbar } from '~/components/ui/views-toolbar';
import { useSortParams } from '~/hooks/use-sort-params';
import { useUptimeTable } from '~/hooks/use-uptime-table';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { listUptimeChecksOptions } from '~/queries/uptime';

type StatusView = 'all' | 'up' | 'down' | 'degraded' | 'paused';

const VIEWS: { value: StatusView; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'up', label: 'Up' },
  { value: 'down', label: 'Down' },
  { value: 'degraded', label: 'Degraded' },
  { value: 'paused', label: 'Paused' },
];

function isStatusView(value: string | null): value is StatusView {
  return VIEWS.some((view) => view.value === value);
}

export function meta() {
  return [{ title: 'Uptime · Daroyan' }];
}

export default function UptimePage() {
  const isAdmin = useIsAdmin();
  const { data: checks, isLoading } = useQuery(listUptimeChecksOptions());
  const [searchParams, setSearchParams] = useSearchParams();
  const viewParam = searchParams.get('view');
  const view: StatusView = isStatusView(viewParam) ? viewParam : 'all';
  const search = searchParams.get('q') ?? '';

  const { sorting, onSortingChange } = useSortParams({
    sorts: { name: 'asc', uptime: 'asc', response: 'desc', lastCheck: 'desc' },
    defaultSort: 'name',
  });

  const setParam = (key: string, value: string | null) =>
    setSearchParams(
      (params) => {
        if (value) {
          params.set(key, value);
        } else {
          params.delete(key);
        }
        return params;
      },
      { replace: true }
    );

  const term = search.trim().toLowerCase();
  const visible = (checks ?? []).filter(
    (check) =>
      (view === 'all' || check.status === view) &&
      (!term || check.name.toLowerCase().includes(term) || check.target.toLowerCase().includes(term))
  );

  const table = useUptimeTable({ checks: visible, sorting, onSortingChange });
  const isEmpty = checks?.length === 0;

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar icon="activity" title="Uptime">
        {!isEmpty && isAdmin && (
          <Button nativeButton={false} render={<Link to="/uptime/new" />} className="w-fit">
            <Icon name="plus" />
            New Check
          </Button>
        )}
      </PageBar>

      {isEmpty ? (
        <EmptyState
          icon="activity"
          marks={['check', 'check', 'clock']}
          title="Add your first uptime check"
          description="Daroyan calls a URL or opens a port on a schedule, and tells you when it stops answering."
        >
          {isAdmin && (
            <Button nativeButton={false} render={<Link to="/uptime/new" />} className="w-fit">
              <Icon name="plus" />
              New Check
            </Button>
          )}
        </EmptyState>
      ) : (
        <div className="flex flex-col gap-2 px-3 pt-1 pb-3">
          <ViewsToolbar
            views={VIEWS}
            view={view}
            onViewChange={(next) => setParam('view', next === 'all' ? null : next)}
            searchLabel="Search by name or URL"
            search={search}
            onSearchChange={(next) => setParam('q', next || null)}
            isViewChanged={view !== 'all' || !!search}
            onReset={() => setSearchParams({}, { replace: true })}
          />
          <DataTable table={table} label="Uptime Checks" size="lg" isLoading={isLoading} />
          {checks && visible.length === 0 && (
            <NoResults title="No checks found" onReset={() => setSearchParams({}, { replace: true })} />
          )}
        </div>
      )}
    </div>
  );
}
