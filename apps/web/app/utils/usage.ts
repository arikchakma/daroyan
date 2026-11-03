import type { UsageTone } from '~/components/servers/usage-ring';

export function getUsageTone(percent: number): UsageTone {
  if (percent >= 90) {
    return 'crit';
  }
  if (percent >= 75) {
    return 'warn';
  }

  return 'ok';
}
