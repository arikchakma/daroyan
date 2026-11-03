import type { Schemas } from '@daroyan/client';

export function getRoleLabel(role: Schemas['Me']['role']) {
  if (role === 'admin') {
    return 'Admin';
  }

  return 'Member';
}
