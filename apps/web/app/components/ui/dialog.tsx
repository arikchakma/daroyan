import { Dialog as DialogPrimitive } from '@base-ui/react/dialog';
import type { ComponentProps } from 'react';

import { Icon } from '~/components/icons/icon';
import { cn } from '~/utils/classname';

import { headingVariants } from './heading';
import type { HeadingVariantProps } from './heading';
import { textVariants } from './text';
import type { TextVariantProps } from './text';

type DialogBackdropProps = DialogPrimitive.Backdrop.Props;

function DialogBackdrop(props: DialogBackdropProps) {
  const { className, ...rest } = props;

  return (
    <DialogPrimitive.Backdrop
      className={cn(
        'fixed inset-0 z-50 bg-black/30 transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0',
        className
      )}
      {...rest}
    />
  );
}

type DialogViewportProps = DialogPrimitive.Viewport.Props;

function DialogViewport(props: DialogViewportProps) {
  const { className, ...rest } = props;

  return (
    <DialogPrimitive.Viewport
      className={cn('fixed inset-0 z-50 overflow-hidden', className)}
      {...rest}
    />
  );
}

type DialogPopupProps = DialogPrimitive.Popup.Props & {
  showCloseButton?: boolean;
  closeButtonClassName?: string;
};

function DialogPopup(props: DialogPopupProps) {
  const {
    className,
    children,
    showCloseButton = true,
    closeButtonClassName,
    ...rest
  } = props;

  return (
    <DialogPrimitive.Popup
      className={cn(
        'fixed top-1/2 left-1/2 z-50 flex w-120 max-w-[calc(100vw-32px)] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default text-text-primary shadow-popover transition-[scale,opacity] duration-150 ease-out outline-none data-ending-style:scale-[0.98] data-ending-style:opacity-0 data-starting-style:scale-[0.98] data-starting-style:opacity-0',
        className
      )}
      {...rest}
    >
      {children}
      {showCloseButton && (
        <DialogPrimitive.Close
          className={cn(
            'absolute top-3 right-2.5 flex size-8 items-center justify-center rounded-full text-text-primary outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 data-disabled:pointer-events-none',
            closeButtonClassName
          )}
        >
          <Icon name="x" />
          <span className="sr-only">Close</span>
        </DialogPrimitive.Close>
      )}
    </DialogPrimitive.Popup>
  );
}

type DialogHeaderProps = ComponentProps<'div'>;

function DialogHeader(props: DialogHeaderProps) {
  const { className, ...rest } = props;

  return (
    <div
      className={cn('flex flex-col gap-1 pt-4.5 pr-12 pb-1 pl-4', className)}
      {...rest}
    />
  );
}

type DialogBodyProps = ComponentProps<'div'>;

function DialogBody(props: DialogBodyProps) {
  const { className, ...rest } = props;

  return (
    <div
      className={cn('flex flex-col gap-3.5 px-4 pt-2 pb-4', className)}
      {...rest}
    />
  );
}

type DialogFooterProps = ComponentProps<'div'>;

function DialogFooter(props: DialogFooterProps) {
  const { className, ...rest } = props;

  return (
    <div
      className={cn(
        'flex items-center justify-end gap-2 bg-surface-subtle px-4 py-3',
        className
      )}
      {...rest}
    />
  );
}

type DialogTitleProps = DialogPrimitive.Title.Props & HeadingVariantProps;

function DialogTitle(props: DialogTitleProps) {
  const { className, size, weight, ...rest } = props;

  return (
    <DialogPrimitive.Title
      className={cn(
        (size || weight) && headingVariants({ size, weight }),
        'text-body font-semibold',
        className
      )}
      {...rest}
    />
  );
}

type DialogDescriptionProps = DialogPrimitive.Description.Props &
  TextVariantProps;

function DialogDescription(props: DialogDescriptionProps) {
  const { className, size, weight, ...rest } = props;

  return (
    <DialogPrimitive.Description
      className={cn(
        (size || weight) && textVariants({ size, weight }),
        'text-body text-text-primary',
        className
      )}
      {...rest}
    />
  );
}

export const Dialog = {
  Root: DialogPrimitive.Root,
  Trigger: DialogPrimitive.Trigger,
  Portal: DialogPrimitive.Portal,
  Close: DialogPrimitive.Close,
  Backdrop: DialogBackdrop,
  Viewport: DialogViewport,
  Popup: DialogPopup,
  Header: DialogHeader,
  Body: DialogBody,
  Footer: DialogFooter,
  Title: DialogTitle,
  Description: DialogDescription,
};
