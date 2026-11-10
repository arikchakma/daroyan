import { keepPreviousData, mutationOptions, queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

const REFRESH_MS = 30_000;

export type TraceFilters = {
  rangeMs: number;
  service?: string;
  server_id?: string;
  status?: string;
  min_duration_ms?: number;
  q?: string;
  offset?: number;
  limit?: number;
};

export const traceQueries = {
  all: () => ['traces'] as const,
  list: (filters: TraceFilters) =>
    queryOptions({
      queryKey: [...traceQueries.all(), 'list', filters],
      queryFn: () => {
        const { rangeMs, ...query } = filters;
        return request(apiClient.GET('/v1/traces', { params: { query: { ...query, since: Date.now() - rangeMs } } }));
      },
      placeholderData: keepPreviousData,
      refetchInterval: REFRESH_MS,
    }),
  detail: (traceId: string, at?: number) =>
    queryOptions({
      queryKey: [...traceQueries.all(), 'detail', traceId],
      queryFn: () =>
        request(apiClient.GET('/v1/traces/{trace_id}', { params: { path: { trace_id: traceId }, query: { at } } })),
    }),
  services: (rangeMs: number) =>
    queryOptions({
      queryKey: [...traceQueries.all(), 'services', rangeMs],
      queryFn: () =>
        request(apiClient.GET('/v1/traces/services', { params: { query: { since: Date.now() - rangeMs } } })),
      refetchInterval: REFRESH_MS,
    }),
  status: (refetchInterval?: number) =>
    queryOptions({
      queryKey: [...traceQueries.all(), 'status'],
      queryFn: () => request(apiClient.GET('/v1/traces/status')),
      refetchInterval,
    }),
};

export const issueQueries = {
  all: () => ['issues'] as const,
  list: (since?: number) =>
    queryOptions({
      queryKey: [...issueQueries.all(), 'list', since],
      queryFn: () => request(apiClient.GET('/v1/issues', { params: { query: { since } } })),
      refetchInterval: REFRESH_MS,
    }),
  events: (fingerprint: string) =>
    queryOptions({
      queryKey: [...issueQueries.all(), 'events', fingerprint],
      queryFn: () =>
        request(apiClient.GET('/v1/issues/{fingerprint}/events', { params: { path: { fingerprint } } })),
      refetchInterval: REFRESH_MS,
    }),
};

export const ingestTokenQueries = {
  all: () => ['ingest-tokens'] as const,
  list: () =>
    queryOptions({
      queryKey: [...ingestTokenQueries.all(), 'list'],
      queryFn: () => request(apiClient.GET('/v1/ingest-tokens')),
    }),
};

export function createIngestTokenOptions() {
  return mutationOptions({
    mutationFn: (body: { name: string }) => request(apiClient.POST('/v1/ingest-tokens', { body })),
  });
}

export function deleteIngestTokenOptions() {
  return mutationOptions({
    mutationFn: (variables: { tokenId: string }) =>
      request(
        apiClient.DELETE('/v1/ingest-tokens/{token_id}', { params: { path: { token_id: variables.tokenId } } })
      ),
  });
}
