import { mutationOptions, queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

const REFRESH_MS = 30_000;

export type TraceFilters = {
  rangeMs: number;
  service?: string;
  status?: string;
  min_duration_ms?: number;
  q?: string;
  offset?: number;
  limit?: number;
};

export function listTracesOptions(filters: TraceFilters) {
  return queryOptions({
    queryKey: ['traces', filters],
    queryFn: () => {
      const { rangeMs, ...query } = filters;
      return request(apiClient.GET('/v1/traces', { params: { query: { ...query, since: Date.now() - rangeMs } } }));
    },
    refetchInterval: REFRESH_MS,
  });
}

export function traceOptions(traceId: string) {
  return queryOptions({
    queryKey: ['traces', 'detail', traceId],
    queryFn: () => request(apiClient.GET('/v1/traces/{trace_id}', { params: { path: { trace_id: traceId } } })),
  });
}

export function traceServicesOptions(rangeMs: number) {
  return queryOptions({
    queryKey: ['traces', 'services', rangeMs],
    queryFn: () =>
      request(apiClient.GET('/v1/traces/services', { params: { query: { since: Date.now() - rangeMs } } })),
    refetchInterval: REFRESH_MS,
  });
}

export function traceStatusOptions(refetchInterval?: number) {
  return queryOptions({
    queryKey: ['traces', 'status'],
    queryFn: () => request(apiClient.GET('/v1/traces/status')),
    refetchInterval,
  });
}

export function listIssuesOptions(since?: number) {
  return queryOptions({
    queryKey: ['issues', since],
    queryFn: () => request(apiClient.GET('/v1/issues', { params: { query: { since } } })),
    refetchInterval: REFRESH_MS,
  });
}

export function issueEventsOptions(fingerprint: string) {
  return queryOptions({
    queryKey: ['issues', 'events', fingerprint],
    queryFn: () =>
      request(apiClient.GET('/v1/issues/{fingerprint}/events', { params: { path: { fingerprint } } })),
    refetchInterval: REFRESH_MS,
  });
}

export function listIngestTokensOptions() {
  return queryOptions({
    queryKey: ['ingest-tokens'],
    queryFn: () => request(apiClient.GET('/v1/ingest-tokens')),
  });
}

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
