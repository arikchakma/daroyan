import { Select as SelectPrimitive } from '@base-ui/react/select';

import { Icon } from '~/components/icons/icon';
import { cn } from '~/utils/classname';

type SelectTriggerProps = SelectPrimitive.Trigger.Props;

function SelectTrigger(props: SelectTriggerProps) {
  const { className, children, ...rest } = props;

  return (
    <SelectPrimitive.Trigger
      className={cn(
        'flex h-9 w-full items-center justify-between gap-2 rounded-lg border border-border-ring bg-surface-default pr-2.5 pl-3 text-body whitespace-nowrap text-text-primary outline-none focus-visible:border-border-strong data-disabled:cursor-not-allowed data-disabled:opacity-60 data-popup-open:border-border-strong',
        className
      )}
      {...rest}
    >
      {children}
      <SelectPrimitive.Icon className="flex text-icon-secondary">
        <Icon name="chevrons-updown" />
      </SelectPrimitive.Icon>
    </SelectPrimitive.Trigger>
  );
}

type SelectValueProps = SelectPrimitive.Value.Props;

function SelectValue(props: SelectValueProps) {
  const { className, ...rest } = props;

  return (
    <SelectPrimitive.Value
      className={cn('truncate data-placeholder:text-text-tertiary', className)}
      {...rest}
    />
  );
}

type SelectPositionerProps = SelectPrimitive.Positioner.Props;

function SelectPositioner(props: SelectPositionerProps) {
  const { className, sideOffset = 4, alignItemWithTrigger = false, ...rest } = props;

  return (
    <SelectPrimitive.Positioner
      className={cn('z-50 outline-none select-none', className)}
      sideOffset={sideOffset}
      alignItemWithTrigger={alignItemWithTrigger}
      {...rest}
    />
  );
}

type SelectPopupProps = SelectPrimitive.Popup.Props;

function SelectPopup(props: SelectPopupProps) {
  const { className, ...rest } = props;

  return (
    <SelectPrimitive.Popup
      className={cn(
        'flex max-h-(--available-height) min-w-(--anchor-width) origin-(--transform-origin) flex-col gap-px overflow-y-auto rounded-lg border border-border-ring bg-surface-default p-1 text-text-primary shadow-popover transition-[scale,opacity] duration-100 ease-out outline-none data-ending-style:scale-[0.98] data-ending-style:opacity-0 data-starting-style:scale-[0.98] data-starting-style:opacity-0',
        className
      )}
      {...rest}
    />
  );
}

type SelectItemProps = SelectPrimitive.Item.Props;

function SelectItem(props: SelectItemProps) {
  const { className, children, ...rest } = props;

  return (
    <SelectPrimitive.Item
      className={cn(
        'relative flex h-8 cursor-default items-center gap-2.5 rounded-sm px-2 text-body text-text-primary outline-none select-none data-disabled:pointer-events-none data-disabled:text-text-tertiary data-highlighted:bg-surface-fill data-selected:bg-surface-fill',
        className
      )}
      {...rest}
    >
      <SelectPrimitive.ItemText className="min-w-0 flex-1 truncate">{children}</SelectPrimitive.ItemText>
      <SelectPrimitive.ItemIndicator className="flex">
        <Icon name="check" />
      </SelectPrimitive.ItemIndicator>
    </SelectPrimitive.Item>
  );
}

type SelectSeparatorProps = SelectPrimitive.Separator.Props;

function SelectSeparator(props: SelectSeparatorProps) {
  const { className, ...rest } = props;

  return <SelectPrimitive.Separator className={cn('my-1 h-px bg-border-subtle', className)} {...rest} />;
}

type SelectGroupLabelProps = SelectPrimitive.GroupLabel.Props;

function SelectGroupLabel(props: SelectGroupLabelProps) {
  const { className, ...rest } = props;

  return (
    <SelectPrimitive.GroupLabel
      className={cn('px-2 pt-1.5 pb-1 text-caption font-medium text-text-tertiary', className)}
      {...rest}
    />
  );
}

type SelectGroupProps = SelectPrimitive.Group.Props;

function SelectGroup(props: SelectGroupProps) {
  const { className, ...rest } = props;

  return <SelectPrimitive.Group className={cn('flex flex-col gap-px', className)} {...rest} />;
}

export const Select = {
  Root: SelectPrimitive.Root,
  Portal: SelectPrimitive.Portal,
  Group: SelectGroup,
  GroupLabel: SelectGroupLabel,
  Trigger: SelectTrigger,
  Value: SelectValue,
  Positioner: SelectPositioner,
  Popup: SelectPopup,
  Item: SelectItem,
  Separator: SelectSeparator,
};
