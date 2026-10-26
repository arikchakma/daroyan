import { cva } from 'class-variance-authority';
import type { VariantProps } from 'class-variance-authority';
import { createElement } from 'react';
import type { ComponentPropsWithRef } from 'react';

import { cn } from '~/utils/classname';

export const textVariants = cva('m-0 font-normal', {
  variants: {
    size: {
      '1': 'tracking-1 text-xs leading-4',
      '2': 'text-sm leading-5',
      '3': 'text-base leading-6',
      '4': 'tracking-4 text-lg leading-6.5',
      '5': 'tracking-5 text-xl leading-7',
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
  },
});

type SpanProps = { as?: 'span' } & ComponentPropsWithRef<'span'>;
type ParagraphProps = { as: 'p' } & ComponentPropsWithRef<'p'>;
type LabelProps = { as: 'label' } & ComponentPropsWithRef<'label'>;
type DivProps = { as: 'div' } & ComponentPropsWithRef<'div'>;

export type TextVariantProps = VariantProps<typeof textVariants>;
export type TextProps = TextVariantProps &
  (SpanProps | ParagraphProps | LabelProps | DivProps);

export function Text(props: TextProps) {
  const {
    children,
    size = '3',
    weight = 'normal',
    className,
    as = 'span',
    ...rest
  } = props;

  return createElement(
    as,
    {
      className: cn(textVariants({ size, weight }), className),
      ...rest,
    },
    children
  );
}
