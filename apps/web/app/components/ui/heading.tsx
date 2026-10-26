import { cva } from 'class-variance-authority';
import type { VariantProps } from 'class-variance-authority';
import { createElement } from 'react';
import type { ComponentPropsWithRef } from 'react';

import { cn } from '~/utils/classname';

export const headingVariants = cva('font-bold not-italic', {
  variants: {
    size: {
      '1': 'tracking-1 text-xs leading-4',

      '2': 'text-sm leading-4.5',
      '3': 'text-base leading-5.5',
      '4': 'tracking-4 text-lg leading-6',

      '5': 'tracking-5 text-xl leading-6.5',
      '6': 'tracking-6 text-2xl leading-7.5',
      '7': 'tracking-7 text-[28px] leading-9',
      '8': 'tracking-8 text-[35px] leading-10',
      '9': 'tracking-9 text-6xl leading-12',
    },
    weight: {
      light: 'font-light',
      normal: 'font-normal',
      medium: 'font-medium',
      semibold: 'font-semibold',
      bold: 'font-bold',
    },
  },
  defaultVariants: {
    size: '3',
    weight: 'normal',
  },
});

export type HeadingVariantProps = VariantProps<typeof headingVariants>;
export interface HeadingProps
  extends ComponentPropsWithRef<'h1'>, HeadingVariantProps {
  as?: 'h1' | 'h2' | 'h3' | 'h4' | 'h5' | 'h6';
}

export function Heading(props: HeadingProps) {
  const {
    children,
    size = '3',
    className,
    as = 'h1',
    weight = 'bold',
    ...rest
  } = props;

  return createElement(
    as,
    {
      className: cn(headingVariants({ size, weight }), className),
      ...rest,
    },
    children
  );
}
