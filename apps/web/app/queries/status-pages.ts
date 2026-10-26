import type { Schemas } from '@daroyan/client';
import { mutationOptions, queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

const REFRESH_MS = 30_000;

export type StatusPageInput = Schemas['StatusPageBody'];

export function listStatusPagesOptions() {
  return queryOptions({
    queryKey: ['status-pages'],
    queryFn: () => request(apiClient.GET('/v1/status-pages')),
    refetchInterval: REFRESH_MS,
  });
}

export function statusPageOptions(pageId: string) {
  return queryOptions({
    queryKey: ['status-pages', pageId],
    queryFn: () =>
      request(apiClient.GET('/v1/status-pages/{page_id}', { params: { path: { page_id: pageId } } })),
  });
}

export function publicStatusPageOptions(slug: string) {
  return queryOptions({
    queryKey: ['status', slug],
    queryFn: () => request(apiClient.GET('/v1/status/{slug}', { params: { path: { slug } } })),
    refetchInterval: REFRESH_MS,
  });
}

export function publicIncidentOptions(slug: string, incidentId: string) {
  return queryOptions({
    queryKey: ['status', slug, 'incidents', incidentId],
    queryFn: () =>
      request(
        apiClient.GET('/v1/status/{slug}/incidents/{incident_id}', {
          params: { path: { slug, incident_id: incidentId } },
        })
      ),
    refetchInterval: REFRESH_MS,
  });
}

export function createStatusPageOptions() {
  return mutationOptions({
    mutationFn: (body: StatusPageInput) => request(apiClient.POST('/v1/status-pages', { body })),
  });
}

export function updateStatusPageOptions() {
  return mutationOptions({
    mutationFn: (variables: { pageId: string; body: StatusPageInput }) =>
      request(
        apiClient.PUT('/v1/status-pages/{page_id}', {
          params: { path: { page_id: variables.pageId } },
          body: variables.body,
        })
      ),
  });
}

export function deleteStatusPageOptions() {
  return mutationOptions({
    mutationFn: (variables: { pageId: string }) =>
      request(apiClient.DELETE('/v1/status-pages/{page_id}', { params: { path: { page_id: variables.pageId } } })),
  });
}

export function toStatusPageInput(page: Schemas['StatusPage']): StatusPageInput {
  return {
    name: page.name,
    slug: page.slug,
    logo_url: page.logo_url,
    is_public: page.is_public,
    sections: page.sections,
  };
}
