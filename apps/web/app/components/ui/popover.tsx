import { Popover as PopoverPrimitive } from '@base-ui/react/popover';

import { cn } from '~/utils/classname';

import { headingVariants } from './heading';
import type { HeadingVariantProps } from './heading';
import { textVariants } from './text';
import type { TextVariantProps } from './text';

type PopoverPositionerProps = PopoverPrimitive.Positioner.Props;

function PopoverPositioner(props: PopoverPositionerProps) {
  const { className, sideOffset = 4, ...rest } = props;

  return (
    <PopoverPrimitive.Positioner
      className={cn('z-50 outline-none', className)}
      sideOffset={sideOffset}
      {...rest}
    />
  );
}

type PopoverPopupProps = PopoverPrimitive.Popup.Props;

function PopoverPopup(props: PopoverPopupProps) {
  const { className, ...rest } = props;

  return (
    <PopoverPrimitive.Popup
      className={cn(
        'w-72 origin-(--transform-origin) rounded-lg border border-border-ring bg-surface-default p-4 text-text-primary shadow-popover transition-[scale,opacity] duration-100 ease-out outline-none data-ending-style:scale-[0.98] data-ending-style:opacity-0 data-starting-style:scale-[0.98] data-starting-style:opacity-0',
        className
      )}
      {...rest}
    />
  );
}

type PopoverBackdropProps = PopoverPrimitive.Backdrop.Props;

function PopoverBackdrop(props: PopoverBackdropProps) {
  const { className, ...rest } = props;

  return (
    <PopoverPrimitive.Backdrop
      className={cn(
        'fixed inset-0 z-40 transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0',
        className
      )}
      {...rest}
    />
  );
}

type PopoverTitleProps = PopoverPrimitive.Title.Props & HeadingVariantProps;

function PopoverTitle(props: PopoverTitleProps) {
  const { className, size = '2', weight = 'medium', ...rest } = props;

  return (
    <PopoverPrimitive.Title
      className={cn(
        headingVariants({ size, weight }),
        'leading-none tracking-tight',
        className
      )}
      {...rest}
    />
  );
}

type PopoverDescriptionProps = PopoverPrimitive.Description.Props &
  TextVariantProps;

function PopoverDescription(props: PopoverDescriptionProps) {
  const { className, size = '2', weight = 'normal', ...rest } = props;

  return (
    <PopoverPrimitive.Description
      className={cn(
        textVariants({ size, weight }),
        'text-balance text-text-tertiary',
        className
      )}
      {...rest}
    />
  );
}

export const Popover = {
  Root: PopoverPrimitive.Root,
  Trigger: PopoverPrimitive.Trigger,
  Portal: PopoverPrimitive.Portal,
  Arrow: PopoverPrimitive.Arrow,
  Viewport: PopoverPrimitive.Viewport,
  Close: PopoverPrimitive.Close,
  Backdrop: PopoverBackdrop,
  Positioner: PopoverPositioner,
  Popup: PopoverPopup,
  Title: PopoverTitle,
  Description: PopoverDescription,
};
