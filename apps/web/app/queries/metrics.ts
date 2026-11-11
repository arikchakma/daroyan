import { keepPreviousData, queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';
import type { MetricAggregation } from '~/utils/metrics';

const REFRESH_MS = 30_000;

export type MetricFilters = {
  name: string;
  rangeMs: number;
  agg?: MetricAggregation;
  service?: string;
  server_id?: string;
  filter?: string;
  group_by?: string;
  step_ms?: number;
};

export const metricQueries = {
  all: () => ['metrics'] as const,
  list: (rangeMs?: number) =>
    queryOptions({
      queryKey: [...metricQueries.all(), 'list', rangeMs],
      queryFn: () =>
        request(
          apiClient.GET('/v1/metrics', {
            params: { query: { since: rangeMs ? Date.now() - rangeMs : undefined } },
          })
        ),
      refetchInterval: REFRESH_MS,
    }),
  attributes: (name: string) =>
    queryOptions({
      queryKey: [...metricQueries.all(), 'attributes', name],
      queryFn: () =>
        request(apiClient.GET('/v1/metrics/{name}/attributes', { params: { path: { name } } })),
    }),
  query: (filters: MetricFilters) =>
    queryOptions({
      queryKey: [...metricQueries.all(), 'query', filters],
      queryFn: () => {
        const { name, rangeMs, ...query } = filters;
        return request(
          apiClient.GET('/v1/metrics/{name}/query', {
            params: { path: { name }, query: { ...query, since: Date.now() - rangeMs } },
          })
        );
      },
      placeholderData: keepPreviousData,
      refetchInterval: REFRESH_MS,
    }),
};
