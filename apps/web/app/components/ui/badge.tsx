import { cva } from 'class-variance-authority';
import type { VariantProps } from 'class-variance-authority';
import type { ReactNode } from 'react';

import { cn } from '~/utils/classname';

const badgeVariants = cva(
  'inline-flex h-5.5 w-fit items-center gap-1 rounded-sm pr-2 pl-1.5 text-caption font-medium whitespace-nowrap [&_svg]:size-3.5 [&_svg]:shrink-0',
  {
    variants: {
      tone: {
        neutral: 'bg-tone-neutral-bg text-tone-neutral-fg',
        success: 'bg-tone-success-bg text-tone-success-fg',
        info: 'bg-tone-info-bg text-tone-info-fg',
        attention: 'bg-tone-attention-bg text-tone-attention-fg',
        warning: 'bg-tone-warning-bg text-tone-warning-fg',
        critical: 'bg-tone-critical-bg text-tone-critical-fg',
        magic: 'bg-tone-magic-bg text-tone-magic-fg',
      },
    },
    defaultVariants: {
      tone: 'neutral',
    },
  }
);

export type BadgeProps = VariantProps<typeof badgeVariants> & {
  children: ReactNode;
  className?: string;
};

export function Badge(props: BadgeProps) {
  const { tone, children, className } = props;

  return (
    <span className={cn(badgeVariants({ tone }), className)}>{children}</span>
  );
}
