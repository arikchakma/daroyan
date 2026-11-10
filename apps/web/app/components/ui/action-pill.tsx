import { cva } from 'class-variance-authority';
import type { VariantProps } from 'class-variance-authority';
import type { ReactNode } from 'react';

import { Icon } from '~/components/icons/icon';
import type { IconName } from '~/components/icons/icon-paths';

const actionPill = cva(
  'flex h-7 items-center gap-1.5 rounded-full pr-3 pl-2 text-body font-medium outline-none focus-visible:ring-2 focus-visible:ring-text-link/60 disabled:opacity-40 [&_svg]:size-3.5',
  {
    variants: {
      tone: {
        dark: 'bg-sidebar-item-active text-text-on-dark',
        light: 'bg-surface-default text-text-primary',
      },
    },
  }
);

type ActionPillProps = VariantProps<typeof actionPill> & {
  icon: IconName;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
};

export function ActionPill(props: ActionPillProps) {
  const { icon, tone, disabled, onClick, children } = props;

  return (
    <button type="button" disabled={disabled} onClick={onClick} className={actionPill({ tone })}>
      <Icon name={icon} />
      {children}
    </button>
  );
}
