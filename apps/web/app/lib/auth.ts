import { apiClient } from './api-client';
import { queryClient } from './query-client';

export function isLoggedIn() {
  return document.cookie.split('; ').includes('daroyan_signed_in=1');
}

export async function logout() {
  await apiClient.POST('/v1/auth/logout').catch(() => {});
  queryClient.clear();
  window.location.href = '/login';
}
