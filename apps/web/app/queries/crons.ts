import type { Schemas } from '@daroyan/client';
import { keepPreviousData, mutationOptions, queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

const REFRESH_MS = 15_000;

export type CronInput = Schemas['CronBody'];

export const cronQueries = {
  all: () => ['crons'] as const,
  list: () =>
    queryOptions({
      queryKey: [...cronQueries.all(), 'list'],
      queryFn: () => request(apiClient.GET('/v1/crons')),
      refetchInterval: REFRESH_MS,
    }),
  detail: (cronId: string) =>
    queryOptions({
      queryKey: [...cronQueries.all(), 'detail', cronId],
      queryFn: () => request(apiClient.GET('/v1/crons/{cron_id}', { params: { path: { cron_id: cronId } } })),
      refetchInterval: REFRESH_MS,
    }),
  stats: (cronId: string) =>
    queryOptions({
      queryKey: [...cronQueries.detail(cronId).queryKey, 'stats'],
      queryFn: () => request(apiClient.GET('/v1/crons/{cron_id}/stats', { params: { path: { cron_id: cronId } } })),
      refetchInterval: REFRESH_MS,
    }),
  preview: (expression: string, timeZone: string) =>
    queryOptions({
      queryKey: [...cronQueries.all(), 'preview', expression, timeZone],
      queryFn: () =>
        request(apiClient.GET('/v1/crons/preview', { params: { query: { expression, time_zone: timeZone } } })),
      enabled: expression.trim().length > 0,
      placeholderData: keepPreviousData,
      retry: false,
    }),
};

export function createCronOptions() {
  return mutationOptions({
    mutationFn: (body: CronInput) => request(apiClient.POST('/v1/crons', { body })),
  });
}

export function updateCronOptions() {
  return mutationOptions({
    mutationFn: (variables: { cronId: string; body: CronInput }) =>
      request(
        apiClient.PUT('/v1/crons/{cron_id}', { params: { path: { cron_id: variables.cronId } }, body: variables.body })
      ),
  });
}

export function deleteCronOptions() {
  return mutationOptions({
    mutationFn: (variables: { cronId: string }) =>
      request(apiClient.DELETE('/v1/crons/{cron_id}', { params: { path: { cron_id: variables.cronId } } })),
  });
}

export function toCronInput(cron: Schemas['Cron']): CronInput {
  return {
    name: cron.name,
    schedule_kind: cron.schedule_kind,
    expression: cron.expression,
    interval_secs: cron.interval_secs,
    time_zone: cron.time_zone,
    grace_secs: cron.grace_secs,
    max_duration_secs: cron.max_duration_secs,
    server_id: cron.server_id,
    command: cron.command,
    paused: cron.paused,
  };
}
