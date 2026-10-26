import type { Schemas } from '@daroyan/client';
import { mutationOptions, queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

const REFRESH_MS = 30_000;

export type IncidentInput = Schemas['IncidentBody'];
export type NewIncidentInput = Schemas['NewIncidentBody'];

export function listIncidentsOptions() {
  return queryOptions({
    queryKey: ['incidents'],
    queryFn: () => request(apiClient.GET('/v1/incidents')),
    refetchInterval: REFRESH_MS,
  });
}

export function incidentOptions(incidentId: string) {
  return queryOptions({
    queryKey: ['incidents', incidentId],
    queryFn: () =>
      request(apiClient.GET('/v1/incidents/{incident_id}', { params: { path: { incident_id: incidentId } } })),
    refetchInterval: REFRESH_MS,
  });
}

export function incidentUpdatesOptions(incidentId: string) {
  return queryOptions({
    queryKey: ['incidents', incidentId, 'updates'],
    queryFn: () =>
      request(
        apiClient.GET('/v1/incidents/{incident_id}/updates', { params: { path: { incident_id: incidentId } } })
      ),
    refetchInterval: REFRESH_MS,
  });
}

export function listUsersOptions() {
  return queryOptions({
    queryKey: ['users'],
    queryFn: () => request(apiClient.GET('/v1/users')),
    staleTime: 60_000,
  });
}

export function createIncidentOptions() {
  return mutationOptions({
    mutationFn: (body: NewIncidentInput) => request(apiClient.POST('/v1/incidents', { body })),
  });
}

export function updateIncidentOptions() {
  return mutationOptions({
    mutationFn: (variables: { incidentId: string; body: IncidentInput }) =>
      request(
        apiClient.PUT('/v1/incidents/{incident_id}', {
          params: { path: { incident_id: variables.incidentId } },
          body: variables.body,
        })
      ),
  });
}

export function deleteIncidentOptions() {
  return mutationOptions({
    mutationFn: (variables: { incidentId: string }) =>
      request(
        apiClient.DELETE('/v1/incidents/{incident_id}', { params: { path: { incident_id: variables.incidentId } } })
      ),
  });
}

export function postIncidentUpdateOptions() {
  return mutationOptions({
    mutationFn: (variables: { incidentId: string; body: Schemas['IncidentUpdateBody'] }) =>
      request(
        apiClient.POST('/v1/incidents/{incident_id}/updates', {
          params: { path: { incident_id: variables.incidentId } },
          body: variables.body,
        })
      ),
  });
}

export function editIncidentUpdateOptions() {
  return mutationOptions({
    mutationFn: (variables: { incidentId: string; updateId: string; body: Schemas['EditUpdateBody'] }) =>
      request(
        apiClient.PUT('/v1/incidents/{incident_id}/updates/{update_id}', {
          params: { path: { incident_id: variables.incidentId, update_id: variables.updateId } },
          body: variables.body,
        })
      ),
  });
}

export function savePostmortemOptions() {
  return mutationOptions({
    mutationFn: (variables: { incidentId: string; body: Schemas['PostmortemBody'] }) =>
      request(
        apiClient.PUT('/v1/incidents/{incident_id}/postmortem', {
          params: { path: { incident_id: variables.incidentId } },
          body: variables.body,
        })
      ),
  });
}

export function publishPostmortemOptions() {
  return mutationOptions({
    mutationFn: (variables: { incidentId: string; body: { notify: boolean } }) =>
      request(
        apiClient.POST('/v1/incidents/{incident_id}/postmortem', {
          params: { path: { incident_id: variables.incidentId } },
          body: variables.body,
        })
      ),
  });
}

export function unpublishPostmortemOptions() {
  return mutationOptions({
    mutationFn: (variables: { incidentId: string }) =>
      request(
        apiClient.DELETE('/v1/incidents/{incident_id}/postmortem', {
          params: { path: { incident_id: variables.incidentId } },
        })
      ),
  });
}

export function toIncidentInput(incident: Schemas['Incident']): IncidentInput {
  return {
    title: incident.title,
    severity: incident.severity,
    lead_id: incident.lead?.id ?? null,
    started_at: incident.started_at,
    starts_at: incident.starts_at,
    ends_at: incident.ends_at,
    auto_start: incident.auto_start,
    auto_finish: incident.auto_finish,
    remind_before: incident.remind_before,
    components: incident.components.map(({ kind, id, status }) => ({ kind, id, status })),
    related: incident.related.map(({ kind, id }) => ({ kind, id })),
    responders: incident.responders.map(({ id, role }) => ({ user_id: id, role })),
    channel_ids: incident.channel_ids,
    hidden_page_ids: incident.status_pages.filter((page) => !page.is_shown).map((page) => page.id),
  };
}
