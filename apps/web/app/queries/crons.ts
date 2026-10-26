import type { Schemas } from '@daroyan/client';
import { keepPreviousData, mutationOptions, queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

const REFRESH_MS = 15_000;

export type CronInput = Schemas['CronBody'];

export function listCronsOptions() {
  return queryOptions({
    queryKey: ['crons'],
    queryFn: () => request(apiClient.GET('/v1/crons')),
    refetchInterval: REFRESH_MS,
  });
}

export function cronOptions(cronId: string) {
  return queryOptions({
    queryKey: ['crons', cronId],
    queryFn: () => request(apiClient.GET('/v1/crons/{cron_id}', { params: { path: { cron_id: cronId } } })),
    refetchInterval: REFRESH_MS,
  });
}

export function cronStatsOptions(cronId: string) {
  return queryOptions({
    queryKey: ['crons', cronId, 'stats'],
    queryFn: () => request(apiClient.GET('/v1/crons/{cron_id}/stats', { params: { path: { cron_id: cronId } } })),
    refetchInterval: REFRESH_MS,
  });
}

export function cronPreviewOptions(expression: string, timeZone: string) {
  return queryOptions({
    queryKey: ['crons', 'preview', expression, timeZone],
    queryFn: () => request(apiClient.GET('/v1/crons/preview', { params: { query: { expression, time_zone: timeZone } } })),
    enabled: expression.trim().length > 0,
    placeholderData: keepPreviousData,
    retry: false,
  });
}

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
