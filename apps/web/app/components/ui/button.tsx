import { Button as ButtonPrimitive } from '@base-ui/react/button';
import { cva } from 'class-variance-authority';
import type { VariantProps } from 'class-variance-authority';

import { cn } from '~/utils/classname';

const buttonVariants = cva(
  'inline-flex w-full items-center justify-center gap-1.5 rounded-full text-body font-medium whitespace-nowrap outline-none focus-visible:ring-2 focus-visible:ring-text-link/40 disabled:cursor-not-allowed disabled:opacity-60 data-disabled:cursor-not-allowed data-disabled:opacity-60 [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0',
  {
    variants: {
      variant: {
        primary:
          'bg-button-primary text-text-on-primary shadow-button-primary hover:bg-button-primary/90',
        secondary:
          'border border-border-ring bg-surface-default text-text-primary shadow-button-secondary hover:bg-surface-subtle',
        tertiary:
          'bg-surface-fill text-text-primary hover:bg-surface-fill-strong',
        plain: 'text-text-primary hover:bg-surface-fill',
        critical:
          'bg-tone-critical-fg text-text-on-primary shadow-button-primary hover:bg-tone-critical-fg/90',
        link: 'h-auto w-fit rounded-none px-0 text-text-link hover:underline',
      },
      size: {
        default: 'h-8 px-3',
        lg: 'h-9 px-3',
        xs: 'h-7 w-fit px-2.5',
        icon: 'size-8 shrink-0 px-0',
      },
    },
    defaultVariants: {
      variant: 'primary',
      size: 'default',
    },
  }
);

export type ButtonVariantProps = VariantProps<typeof buttonVariants>;
export type ButtonProps = ButtonPrimitive.Props & ButtonVariantProps;

export function Button(props: ButtonProps) {
  const { className, variant, size, ...rest } = props;

  return (
    <ButtonPrimitive
      className={cn(buttonVariants({ variant, size, className }))}
      {...rest}
    />
  );
}

export { buttonVariants };
