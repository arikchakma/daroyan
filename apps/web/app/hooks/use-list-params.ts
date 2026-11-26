import { useSearchParams } from 'react-router';

export type ParamChanges = Record<string, string | string[] | null>;

export function useListParams() {
  const [searchParams, setSearchParams] = useSearchParams();
  const search = searchParams.get('q') ?? '';

  const setParams = (changes: ParamChanges) =>
    setSearchParams(
      (params) => {
        for (const [key, value] of Object.entries(changes)) {
          params.delete(key);

          if (Array.isArray(value)) {
            value.forEach((item) => params.append(key, item));
          } else if (value) {
            params.set(key, value);
          }
        }
        return params;
      },
      { replace: true }
    );

  const setParam = (key: string, value: string | null) => setParams({ [key]: value });

  const setSearch = (next: string) => setParam('q', next || null);
  const reset = () => setSearchParams({}, { replace: true });

  return { searchParams, search, setParam, setParams, setSearch, reset };
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
