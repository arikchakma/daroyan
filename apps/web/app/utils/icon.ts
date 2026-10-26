import type { IconName } from '~/components/icons/icon-paths';
import { LOGO_PATHS } from '~/components/icons/icon-paths';

export function getIconSymbolId(name: string) {
  return `icon-${name}`;
}

export function isLogoName(name: IconName): name is keyof typeof LOGO_PATHS {
  return name in LOGO_PATHS;
}
