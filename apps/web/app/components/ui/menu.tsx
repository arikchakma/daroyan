import { Menu as MenuPrimitive } from '@base-ui/react/menu';
import type { ComponentProps } from 'react';

import { Icon } from '~/components/icons/icon';
import { cn } from '~/utils/classname';

type MenuPositionerProps = MenuPrimitive.Positioner.Props;

function MenuPositioner(props: MenuPositionerProps) {
  const { className, sideOffset = 4, ...rest } = props;

  return (
    <MenuPrimitive.Positioner
      className={cn('z-50 outline-none', className)}
      sideOffset={sideOffset}
      {...rest}
    />
  );
}

type MenuPopupProps = MenuPrimitive.Popup.Props;

function MenuPopup(props: MenuPopupProps) {
  const { className, ...rest } = props;

  return (
    <MenuPrimitive.Popup
      className={cn(
        'z-50 min-w-50 origin-(--transform-origin) overflow-hidden rounded-lg border border-border-ring bg-surface-default p-1 text-text-primary shadow-popover transition-[scale,opacity] duration-100 ease-out outline-none data-ending-style:scale-[0.98] data-ending-style:opacity-0 data-starting-style:scale-[0.98] data-starting-style:opacity-0',
        className
      )}
      {...rest}
    />
  );
}

type MenuItemProps = MenuPrimitive.Item.Props;

function MenuItem(props: MenuItemProps) {
  const { className, ...rest } = props;

  return (
    <MenuPrimitive.Item
      className={cn(
        'relative flex h-8 cursor-default items-center gap-2.5 rounded-sm px-2 text-body text-text-primary outline-none select-none data-disabled:pointer-events-none data-disabled:text-text-tertiary data-highlighted:bg-surface-fill [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0',
        className
      )}
      {...rest}
    />
  );
}

type MenuLinkItemProps = MenuPrimitive.LinkItem.Props;

function MenuLinkItem(props: MenuLinkItemProps) {
  const { className, ...rest } = props;

  return (
    <MenuPrimitive.LinkItem
      className={cn(
        'relative flex h-8 cursor-default items-center gap-2.5 rounded-sm px-2 text-body text-text-primary outline-none select-none data-disabled:pointer-events-none data-disabled:text-text-tertiary data-highlighted:bg-surface-fill [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0',
        className
      )}
      {...rest}
    />
  );
}

type MenuCheckboxItemProps = MenuPrimitive.CheckboxItem.Props;

function MenuCheckboxItem(props: MenuCheckboxItemProps) {
  const { className, children, ...rest } = props;

  return (
    <MenuPrimitive.CheckboxItem
      className={cn(
        'relative flex h-8 cursor-default items-center gap-2.5 rounded-sm px-2 text-body text-text-primary outline-none select-none data-disabled:pointer-events-none data-disabled:text-text-tertiary data-highlighted:bg-surface-fill [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0',
        className
      )}
      {...rest}
    >
      {children}
      <MenuPrimitive.CheckboxItemIndicator className="ml-auto flex">
        <Icon name="check" />
      </MenuPrimitive.CheckboxItemIndicator>
    </MenuPrimitive.CheckboxItem>
  );
}

type MenuRadioItemProps = MenuPrimitive.RadioItem.Props;

function MenuRadioItem(props: MenuRadioItemProps) {
  const { className, children, ...rest } = props;

  return (
    <MenuPrimitive.RadioItem
      className={cn(
        'relative flex h-8 cursor-default items-center gap-2.5 rounded-sm px-2 text-body text-text-primary outline-none select-none data-disabled:pointer-events-none data-disabled:text-text-tertiary data-highlighted:bg-surface-fill [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0',
        className
      )}
      {...rest}
    >
      {children}
      <MenuPrimitive.RadioItemIndicator className="ml-auto flex">
        <Icon name="check" />
      </MenuPrimitive.RadioItemIndicator>
    </MenuPrimitive.RadioItem>
  );
}

type MenuSubmenuTriggerProps = MenuPrimitive.SubmenuTrigger.Props;

function MenuSubmenuTrigger(props: MenuSubmenuTriggerProps) {
  const { className, children, ...rest } = props;

  return (
    <MenuPrimitive.SubmenuTrigger
      className={cn(
        'relative flex h-8 cursor-default items-center gap-2.5 rounded-sm px-2 text-body text-text-primary outline-none select-none data-disabled:pointer-events-none data-disabled:text-text-tertiary data-highlighted:bg-surface-fill data-popup-open:bg-surface-fill [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0',
        className
      )}
      {...rest}
    >
      {children}
      <Icon name="chevron-right" className="ml-auto" />
    </MenuPrimitive.SubmenuTrigger>
  );
}

type MenuGroupLabelProps = MenuPrimitive.GroupLabel.Props;

function MenuGroupLabel(props: MenuGroupLabelProps) {
  const { className, ...rest } = props;

  return (
    <MenuPrimitive.GroupLabel
      className={cn(
        'px-2 pt-1.5 pb-1 text-caption font-medium text-text-tertiary',
        className
      )}
      {...rest}
    />
  );
}

type MenuSeparatorProps = MenuPrimitive.Separator.Props;

function MenuSeparator(props: MenuSeparatorProps) {
  const { className, ...rest } = props;

  return (
    <MenuPrimitive.Separator
      className={cn('my-1 h-px bg-border-subtle', className)}
      {...rest}
    />
  );
}

type MenuShortcutProps = ComponentProps<'span'>;

function MenuShortcut(props: MenuShortcutProps) {
  const { className, ...rest } = props;

  return (
    <span
      className={cn('ml-auto text-body text-text-tertiary', className)}
      {...rest}
    />
  );
}

export const Menu = {
  Root: MenuPrimitive.Root,
  Trigger: MenuPrimitive.Trigger,
  Portal: MenuPrimitive.Portal,
  Backdrop: MenuPrimitive.Backdrop,
  Group: MenuPrimitive.Group,
  RadioGroup: MenuPrimitive.RadioGroup,
  SubmenuRoot: MenuPrimitive.SubmenuRoot,
  Positioner: MenuPositioner,
  Popup: MenuPopup,
  Item: MenuItem,
  LinkItem: MenuLinkItem,
  CheckboxItem: MenuCheckboxItem,
  RadioItem: MenuRadioItem,
  SubmenuTrigger: MenuSubmenuTrigger,
  GroupLabel: MenuGroupLabel,
  Separator: MenuSeparator,
  Shortcut: MenuShortcut,
};
