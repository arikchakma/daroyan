import type { SortingState } from '@tanstack/react-table';
import { useSearchParams } from 'react-router';

type SortDirection = 'asc' | 'desc';

type UseSortParamsOptions<TSort extends string> = {
  sorts: Record<TSort, SortDirection>;
  defaultSort: NoInfer<TSort>;
};

export function useSortParams<TSort extends string>(
  options: UseSortParamsOptions<TSort>
) {
  const { sorts, defaultSort } = options;

  const [searchParams, setSearchParams] = useSearchParams();
  const sortParam = searchParams.get('sort');
  const sort =
    sortParam && sortParam in sorts ? (sortParam as TSort) : defaultSort;
  const dirParam = searchParams.get('dir');
  const direction: SortDirection =
    dirParam === 'asc' || dirParam === 'desc' ? dirParam : sorts[sort];

  const sorting: SortingState = [{ id: sort, desc: direction === 'desc' }];

  const onSortingChange = (next: SortingState) => {
    const first = next[0];
    if (!first) {
      return;
    }
    const id = first.id as TSort;
    const dir: SortDirection = first.desc ? 'desc' : 'asc';
    setSearchParams(
      (params) => {
        if (id === defaultSort) {
          params.delete('sort');
        } else {
          params.set('sort', id);
        }
        if (dir === sorts[id]) {
          params.delete('dir');
        } else {
          params.set('dir', dir);
        }
        return params;
      },
      { replace: true }
    );
  };

  return { sorting, onSortingChange };
}
