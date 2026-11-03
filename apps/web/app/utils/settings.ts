import type { Schemas } from '@daroyan/client';

export function getRoleLabel(role: Schemas['Me']['role']) {
  if (role === 'admin') {
    return 'Admin';
  }

  return 'Member';
}

export function getOwnersLabel(count: number) {
  if (count === 0) {
    return 'None listed';
  }

  return `${count} listed`;
}
