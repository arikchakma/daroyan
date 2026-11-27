import type { Schemas } from '@daroyan/client';

import type { BadgeProps } from '~/components/ui/badge';

import { pluralize } from '~/utils/format';

export type AccessEvent = Schemas['AccessEvent'];

export type AccessTone = NonNullable<BadgeProps['tone']>;

export function getAccessLabel(event: AccessEvent) {
  const isFailure = event.outcome === 'failure';

  switch (event.action) {
    case 'ssh_login':
      return 'SSH login';
    case 'ssh_logout':
      return 'SSH logout';
    case 'console_login':
      return isFailure ? 'Console login failed' : 'Console login';
    case 'console_logout':
      return 'Console logout';
    case 'sudo':
      return isFailure ? 'sudo denied' : 'sudo';
    case 'su':
      return isFailure ? 'su failed' : 'su';
    case 'user_add':
      return 'User added';
    case 'user_delete':
      return 'User deleted';
    case 'user_change':
      return 'User changed';
    case 'group_add':
      return 'Group added';
    case 'group_delete':
      return 'Group deleted';
    case 'password_change':
      return 'Password changed';
    default:
      return event.action;
  }
}

export function getAccessTone(event: AccessEvent): AccessTone {
  if (event.outcome === 'failure') {
    return 'critical';
  }
  if (event.user === 'root' && (event.action === 'ssh_login' || event.action === 'console_login')) {
    return 'attention';
  }
  return 'neutral';
}

export function isNewAddress(event: AccessEvent) {
  return event.action === 'ssh_login' && event.outcome === 'success' && !!event.ip && event.ip_last_seen == null;
}

function isLoginAction(action: string) {
  return ['ssh_login', 'ssh_logout', 'console_login', 'console_logout'].includes(action);
}

export function getAccessUser(event: AccessEvent) {
  if (event.action === 'sudo' || event.action === 'su') {
    return event.target ? `${event.user ?? 'Someone'} as ${event.target}` : event.user;
  }
  if (isLoginAction(event.action)) {
    return event.user;
  }
  return event.target ?? event.user;
}

export function getAccessDetail(event: AccessEvent) {
  if (event.action === 'ssh_login') {
    return [event.method, event.fingerprint].filter(Boolean).join(' · ') || null;
  }
  if (event.action === 'sudo') {
    return event.command;
  }
  if (event.action === 'console_login' || event.action === 'su') {
    return event.tty;
  }
  if (event.group) {
    return event.user ? `${event.group}, by ${event.user}` : event.group;
  }
  return null;
}

export function getFailedLoginsText(count: number) {
  return `${pluralize(count, 'failed login')} in 24h`;
}
