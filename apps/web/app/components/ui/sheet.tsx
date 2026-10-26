import { Drawer as DrawerPrimitive } from '@base-ui/react/drawer';
import { Icon } from '~/components/icons/icon';
import { createContext, useContext } from 'react';
import type { ComponentProps } from 'react';

import { cn } from '~/utils/classname';

import { headingVariants } from './heading';
import type { HeadingVariantProps } from './heading';
import { textVariants } from './text';
import type { TextVariantProps } from './text';

export type SheetSide = 'top' | 'right' | 'bottom' | 'left';

const SheetSideContext = createContext<SheetSide>('right');

type SheetRootProps = Omit<DrawerPrimitive.Root.Props, 'swipeDirection'> & {
  side?: SheetSide;
};

function SheetRoot(props: SheetRootProps) {
  const { side = 'right', children, ...rest } = props;

  return (
    <SheetSideContext.Provider value={side}>
      <DrawerPrimitive.Root
        swipeDirection={
          side === 'top' ? 'up' : side === 'bottom' ? 'down' : side
        }
        {...rest}
      >
        {children}
      </DrawerPrimitive.Root>
    </SheetSideContext.Provider>
  );
}

type SheetBackdropProps = DrawerPrimitive.Backdrop.Props;

function SheetBackdrop(props: SheetBackdropProps) {
  const { className, ...rest } = props;

  return (
    <DrawerPrimitive.Backdrop
      className={cn(
        'fixed inset-0 z-50 min-h-dvh bg-zinc-950 opacity-[calc(0.5*(1-var(--drawer-swipe-progress)))] transition-opacity duration-150 ease-out data-ending-style:opacity-0 data-starting-style:opacity-0 data-swiping:duration-0',
        className
      )}
      {...rest}
    />
  );
}

type SheetViewportProps = DrawerPrimitive.Viewport.Props;

function SheetViewport(props: SheetViewportProps) {
  const { className, ...rest } = props;
  const side = useContext(SheetSideContext);

  return (
    <DrawerPrimitive.Viewport
      className={cn(
        'fixed inset-0 z-50 flex items-stretch',
        side === 'top' && 'flex-col justify-start',
        side === 'right' && 'flex-row justify-end',
        side === 'bottom' && 'flex-col justify-end',
        side === 'left' && 'flex-row justify-start',
        className
      )}
      {...rest}
    />
  );
}

type SheetPopupProps = DrawerPrimitive.Popup.Props;

function SheetPopup(props: SheetPopupProps) {
  const { className, ...rest } = props;
  const side = useContext(SheetSideContext);

  return (
    <DrawerPrimitive.Popup
      className={cn(
        'flex flex-col gap-4 overflow-y-auto overscroll-contain border-border-default bg-surface-default shadow-lg transition-transform duration-150 ease-out outline-none data-swiping:select-none',
        side === 'top' &&
          'h-auto w-full border-b data-ending-style:-translate-y-full data-starting-style:-translate-y-full',
        side === 'right' &&
          'h-full w-3/4 border-l data-ending-style:translate-x-full data-starting-style:translate-x-full sm:max-w-sm',
        side === 'bottom' &&
          'h-auto w-full border-t data-ending-style:translate-y-full data-starting-style:translate-y-full',
        side === 'left' &&
          'h-full w-3/4 border-r data-ending-style:-translate-x-full data-starting-style:-translate-x-full sm:max-w-sm',
        className
      )}
      {...rest}
    />
  );
}

type SheetCloseButtonProps = DrawerPrimitive.Close.Props;

function SheetCloseButton(props: SheetCloseButtonProps) {
  const { className, ...rest } = props;

  return (
    <DrawerPrimitive.Close
      className={cn(
        'absolute top-4 right-4 rounded-xs opacity-70 transition-opacity hover:opacity-100 focus-visible:ring-2 focus-visible:ring-border-strong focus-visible:ring-offset-2 focus-visible:outline-none data-disabled:pointer-events-none',
        className
      )}
      {...rest}
    >
      <Icon name="x" />
      <span className="sr-only">Close</span>
    </DrawerPrimitive.Close>
  );
}

type SheetHeaderProps = ComponentProps<'div'>;

function SheetHeader(props: SheetHeaderProps) {
  const { className, ...rest } = props;

  return (
    <div className={cn('flex flex-col gap-1.5 p-4', className)} {...rest} />
  );
}

type SheetFooterProps = ComponentProps<'div'>;

function SheetFooter(props: SheetFooterProps) {
  const { className, ...rest } = props;

  return (
    <div
      className={cn('mt-auto flex flex-col gap-2 p-4', className)}
      {...rest}
    />
  );
}

type SheetTitleProps = DrawerPrimitive.Title.Props & HeadingVariantProps;

function SheetTitle(props: SheetTitleProps) {
  const { className, size = '4', weight = 'medium', ...rest } = props;

  return (
    <DrawerPrimitive.Title
      className={cn(
        headingVariants({ size, weight }),
        'leading-none tracking-tight',
        className
      )}
      {...rest}
    />
  );
}

type SheetDescriptionProps = DrawerPrimitive.Description.Props &
  TextVariantProps;

function SheetDescription(props: SheetDescriptionProps) {
  const { className, size = '2', weight = 'normal', ...rest } = props;

  return (
    <DrawerPrimitive.Description
      className={cn(
        textVariants({ size, weight }),
        'text-balance text-text-tertiary',
        className
      )}
      {...rest}
    />
  );
}

export const Sheet = {
  Provider: DrawerPrimitive.Provider,
  Indent: DrawerPrimitive.Indent,
  Trigger: DrawerPrimitive.Trigger,
  Portal: DrawerPrimitive.Portal,
  Content: DrawerPrimitive.Content,
  SwipeArea: DrawerPrimitive.SwipeArea,
  Close: DrawerPrimitive.Close,
  Root: SheetRoot,
  Backdrop: SheetBackdrop,
  Viewport: SheetViewport,
  Popup: SheetPopup,
  CloseButton: SheetCloseButton,
  Header: SheetHeader,
  Footer: SheetFooter,
  Title: SheetTitle,
  Description: SheetDescription,
};
