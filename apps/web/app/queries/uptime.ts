import type { Schemas } from '@daroyan/client';
import { mutationOptions, queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

const REFRESH_MS = 15_000;

export type UptimeCheckInput = Schemas['UptimeCheckBody'];

export function listUptimeChecksOptions() {
  return queryOptions({
    queryKey: ['uptime-checks'],
    queryFn: () => request(apiClient.GET('/v1/uptime-checks')),
    refetchInterval: REFRESH_MS,
  });
}

export function uptimeCheckOptions(checkId: string) {
  return queryOptions({
    queryKey: ['uptime-checks', checkId],
    queryFn: () =>
      request(apiClient.GET('/v1/uptime-checks/{check_id}', { params: { path: { check_id: checkId } } })),
    refetchInterval: REFRESH_MS,
  });
}

export function uptimeCheckStatsOptions(checkId: string) {
  return queryOptions({
    queryKey: ['uptime-checks', checkId, 'stats'],
    queryFn: () =>
      request(apiClient.GET('/v1/uptime-checks/{check_id}/stats', { params: { path: { check_id: checkId } } })),
    refetchInterval: REFRESH_MS,
  });
}

export function createUptimeCheckOptions() {
  return mutationOptions({
    mutationFn: (body: UptimeCheckInput) => request(apiClient.POST('/v1/uptime-checks', { body })),
  });
}

export function updateUptimeCheckOptions() {
  return mutationOptions({
    mutationFn: (variables: { checkId: string; body: UptimeCheckInput }) =>
      request(
        apiClient.PUT('/v1/uptime-checks/{check_id}', {
          params: { path: { check_id: variables.checkId } },
          body: variables.body,
        })
      ),
  });
}

export function deleteUptimeCheckOptions() {
  return mutationOptions({
    mutationFn: (variables: { checkId: string }) =>
      request(
        apiClient.DELETE('/v1/uptime-checks/{check_id}', { params: { path: { check_id: variables.checkId } } })
      ),
  });
}

export function runUptimeCheckOptions() {
  return mutationOptions({
    mutationFn: (variables: { checkId: string }) =>
      request(
        apiClient.POST('/v1/uptime-checks/{check_id}/run', { params: { path: { check_id: variables.checkId } } })
      ),
  });
}

export function toCheckInput(check: Schemas['UptimeCheck']): UptimeCheckInput {
  return {
    name: check.name,
    kind: check.kind,
    target: check.target,
    method: check.method,
    expected_status: check.expected_status,
    body_contains: check.body_contains,
    timeout_secs: check.timeout_secs,
    interval_secs: check.interval_secs,
    paused: check.paused,
  };
}
