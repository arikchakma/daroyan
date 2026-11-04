import { useSearchParams } from 'react-router';

export function useListParams() {
  const [searchParams, setSearchParams] = useSearchParams();
  const search = searchParams.get('q') ?? '';

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

  const setSearch = (next: string) => setParam('q', next || null);
  const reset = () => setSearchParams({}, { replace: true });

  return { searchParams, search, setParam, setSearch, reset };
}

export function useViewParams<TView extends string>(views: readonly TView[]) {
  const params = useListParams();
  const defaultView = views[0];
  const viewParam = params.searchParams.get('view');
  const view = views.find((value) => value === viewParam) ?? defaultView;

  const setView = (next: TView) => params.setParam('view', next === defaultView ? null : next);
  const isFiltered = view !== defaultView || !!params.search;

  return { ...params, view, setView, isFiltered };
}
