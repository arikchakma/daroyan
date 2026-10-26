import { Tooltip as TooltipPrimitive } from '@base-ui/react/tooltip';

import { cn } from '~/utils/classname';

type TooltipPositionerProps = TooltipPrimitive.Positioner.Props;

function TooltipPositioner(props: TooltipPositionerProps) {
  const { className, sideOffset = 6, ...rest } = props;

  return (
    <TooltipPrimitive.Positioner
      className={cn('z-99 outline-none', className)}
      sideOffset={sideOffset}
      {...rest}
    />
  );
}

type TooltipPopupProps = TooltipPrimitive.Popup.Props;

function TooltipPopup(props: TooltipPopupProps) {
  const { className, ...rest } = props;

  return (
    <TooltipPrimitive.Popup
      className={cn(
        'flex origin-(--transform-origin) items-center gap-2 rounded-sm bg-button-primary px-2 py-1 text-caption font-medium text-text-on-primary shadow-tooltip transition-[scale,opacity] duration-100 ease-out data-ending-style:scale-95 data-ending-style:opacity-0 data-instant:transition-none data-starting-style:scale-95 data-starting-style:opacity-0',
        className
      )}
      {...rest}
    />
  );
}

type TooltipArrowProps = TooltipPrimitive.Arrow.Props;

function TooltipArrow(props: TooltipArrowProps) {
  const { className, ...rest } = props;

  return (
    <TooltipPrimitive.Arrow
      className={cn(
        'flex text-button-primary data-[side=bottom]:-top-1.5 data-[side=left]:-right-2.5 data-[side=left]:rotate-90 data-[side=right]:-left-2.5 data-[side=right]:-rotate-90 data-[side=top]:-bottom-1.5 data-[side=top]:rotate-180',
        className
      )}
      {...rest}
    >
      <svg width="14" height="8" viewBox="0 0 14 8" fill="none">
        <path
          d="M6.44291 0.256706L3.20505 4.62782C2.71539 5.28887 2.07993 5.65464 1.42115 5.65464H0V7.65464H13.3333V5.65464H12.3566C11.6979 5.65464 11.0624 5.28887 10.5727 4.62782L7.33487 0.256706C7.08133 -0.0855637 6.69647 -0.0855738 6.44291 0.256706Z"
          fill="currentColor"
        />
      </svg>
    </TooltipPrimitive.Arrow>
  );
}

type TooltipShortcutProps = {
  children: string;
};

function TooltipShortcut(props: TooltipShortcutProps) {
  const { children } = props;

  return (
    <kbd className="-mr-1 rounded-[4px] bg-text-on-primary/14 px-1 py-px font-sans text-axis font-medium">
      {children}
    </kbd>
  );
}

export const Tooltip = {
  Provider: TooltipPrimitive.Provider,
  Root: TooltipPrimitive.Root,
  Trigger: TooltipPrimitive.Trigger,
  Portal: TooltipPrimitive.Portal,
  Arrow: TooltipArrow,
  Viewport: TooltipPrimitive.Viewport,
  Positioner: TooltipPositioner,
  Popup: TooltipPopup,
  Shortcut: TooltipShortcut,
};
