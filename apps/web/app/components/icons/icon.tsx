import { cn } from '~/utils/classname';
import { getIconSymbolId, isLogoName } from '~/utils/icon';

import type { IconName } from './icon-paths';
import { LOGO_PATHS } from './icon-paths';

type IconProps = {
  name: IconName;
  className?: string;
};

export function Icon(props: IconProps) {
  const { name, className } = props;

  const viewBox = isLogoName(name) ? LOGO_PATHS[name].viewBox : '0 0 16 16';

  return (
    <svg
      viewBox={viewBox}
      aria-hidden="true"
      className={cn('size-4 shrink-0', className)}
    >
      <use href={`#${getIconSymbolId(name)}`} />
    </svg>
  );
}
