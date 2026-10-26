import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { DataTable } from '~/components/ui/data-table';
import { EmptyState, NoResults } from '~/components/ui/empty-state';
import { PageBar } from '~/components/ui/page-bar';
import { ViewsToolbar } from '~/components/ui/views-toolbar';
import { useSortParams } from '~/hooks/use-sort-params';
import { useStatusPagesTable } from '~/hooks/use-status-pages-table';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { listStatusPagesOptions } from '~/queries/status-pages';

type VisibilityView = 'all' | 'public' | 'private';

const VIEWS: { value: VisibilityView; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'public', label: 'Public' },
  { value: 'private', label: 'Private' },
];

function isVisibilityView(value: string | null): value is VisibilityView {
  return VIEWS.some((view) => view.value === value);
}

export function meta() {
  return [{ title: 'Status Pages · Daroyan' }];
}

export default function StatusPagesPage() {
  const isAdmin = useIsAdmin();
  const { data: pages, isLoading } = useQuery(listStatusPagesOptions());
  const [searchParams, setSearchParams] = useSearchParams();
  const viewParam = searchParams.get('view');
  const view: VisibilityView = isVisibilityView(viewParam) ? viewParam : 'all';
  const search = searchParams.get('q') ?? '';

  const { sorting, onSortingChange } = useSortParams({
    sorts: { name: 'asc', components: 'desc', updated: 'desc' },
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
  const visible = (pages ?? []).filter(
    (page) =>
      (view === 'all' || page.is_public === (view === 'public')) &&
      (!term || page.name.toLowerCase().includes(term) || page.slug.includes(term))
  );

  const table = useStatusPagesTable({ pages: visible, sorting, onSortingChange });
  const isEmpty = pages?.length === 0;

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar icon="globe" title="Status Pages">
        {!isEmpty && isAdmin && (
          <Button nativeButton={false} render={<Link to="/status-pages/new" />} className="w-fit">
            <Icon name="plus" />
            New Status Page
          </Button>
        )}
      </PageBar>

      {isEmpty ? (
        <EmptyState
          icon="globe"
          marks={['check', 'check', 'check']}
          title="Create your first status page"
          description="Pick the checks and crons to show. Your users get a public page to open when something looks wrong."
        >
          {isAdmin && (
            <Button nativeButton={false} render={<Link to="/status-pages/new" />} className="w-fit">
              <Icon name="plus" />
              New Status Page
            </Button>
          )}
        </EmptyState>
      ) : (
        <div className="flex flex-col gap-2 px-3 pt-1 pb-3">
          <ViewsToolbar
            views={VIEWS}
            view={view}
            onViewChange={(next) => setParam('view', next === 'all' ? null : next)}
            searchLabel="Search by name or slug"
            search={search}
            onSearchChange={(next) => setParam('q', next || null)}
            isViewChanged={view !== 'all' || !!search}
            onReset={() => setSearchParams({}, { replace: true })}
          />
          <DataTable table={table} label="Status Pages" size="lg" isLoading={isLoading} />
          {pages && visible.length === 0 && (
            <NoResults title="No status pages found" onReset={() => setSearchParams({}, { replace: true })} />
          )}
        </div>
      )}
    </div>
  );
}
