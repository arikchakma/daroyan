import { Autocomplete as AutocompletePrimitive } from '@base-ui/react/autocomplete';
import type { ComponentProps, ReactNode } from 'react';

import { cn } from '~/utils/classname';

type CommandInputProps = AutocompletePrimitive.Input.Props;

function CommandInput(props: CommandInputProps) {
  const { className, ...rest } = props;

  return (
    <AutocompletePrimitive.Input
      className={cn(
        'h-5 min-w-0 flex-1 bg-transparent text-[14px] leading-5 tracking-[-0.01em] text-text-primary outline-none placeholder:text-text-tertiary disabled:cursor-not-allowed disabled:opacity-50',
        className
      )}
      {...rest}
    />
  );
}

type CommandViewportProps = {
  className?: string;
  children?: ReactNode;
};

function CommandViewport(props: CommandViewportProps) {
  const { className, children } = props;

  return (
    <div
      className={cn(
        'max-h-[min(60dvh,24rem)] min-h-0 scroll-fade-y scrollbar-none overflow-y-auto overscroll-contain [scroll-padding-block:0.375rem]',
        className
      )}
    >
      {children}
    </div>
  );
}

type CommandListProps = AutocompletePrimitive.List.Props;

function CommandList(props: CommandListProps) {
  const { className, ...rest } = props;

  return (
    <AutocompletePrimitive.List
      className={cn('p-1.5 outline-none data-empty:p-0', className)}
      {...rest}
    />
  );
}

type CommandEmptyProps = AutocompletePrimitive.Empty.Props;

function CommandEmpty(props: CommandEmptyProps) {
  const { className, children, ...rest } = props;

  return (
    <AutocompletePrimitive.Empty {...rest}>
      <div
        className={cn(
          'flex min-h-24 items-center justify-center px-3 text-body text-text-tertiary',
          className
        )}
      >
        {children}
      </div>
    </AutocompletePrimitive.Empty>
  );
}

type CommandItemProps = AutocompletePrimitive.Item.Props;

function CommandItem(props: CommandItemProps) {
  const { className, ...rest } = props;

  return (
    <AutocompletePrimitive.Item
      className={cn(
        'relative flex h-9 cursor-default [scroll-margin-block:0.375rem] items-center gap-2.5 rounded-sm px-2 text-body text-text-primary outline-none select-none data-disabled:pointer-events-none data-disabled:opacity-50 data-highlighted:bg-surface-fill [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0',
        className
      )}
      {...rest}
    />
  );
}

type CommandGroupLabelProps = AutocompletePrimitive.GroupLabel.Props;

function CommandGroupLabel(props: CommandGroupLabelProps) {
  const { className, ...rest } = props;

  return (
    <AutocompletePrimitive.GroupLabel
      className={cn(
        'flex px-2 pt-1.5 pb-1 text-caption font-medium text-text-tertiary select-none',
        className
      )}
      {...rest}
    />
  );
}

type CommandSeparatorProps = AutocompletePrimitive.Separator.Props;

function CommandSeparator(props: CommandSeparatorProps) {
  const { className, ...rest } = props;

  return (
    <AutocompletePrimitive.Separator
      className={cn('my-1 h-px bg-border-row', className)}
      {...rest}
    />
  );
}

type CommandShortcutProps = ComponentProps<'span'>;

function CommandShortcut(props: CommandShortcutProps) {
  const { className, ...rest } = props;

  return (
    <span
      className={cn(
        'ml-auto text-caption font-normal whitespace-nowrap text-text-tertiary',
        className
      )}
      {...rest}
    />
  );
}

export const Command = {
  Root: AutocompletePrimitive.Root,
  Group: AutocompletePrimitive.Group,
  Collection: AutocompletePrimitive.Collection,
  Row: AutocompletePrimitive.Row,
  Status: AutocompletePrimitive.Status,
  Input: CommandInput,
  Viewport: CommandViewport,
  List: CommandList,
  Empty: CommandEmpty,
  Item: CommandItem,
  GroupLabel: CommandGroupLabel,
  Separator: CommandSeparator,
  Shortcut: CommandShortcut,
};
