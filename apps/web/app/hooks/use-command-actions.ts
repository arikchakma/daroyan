import type { IconName } from '~/components/icons/icon-paths';
import { useEffect, useId } from 'react';

export type CommandAction = {
  label: string;
  icon: IconName;
  shortcut?: string;
  keywords?: string;
  isChecked?: boolean;
  onSelect: () => void;
};

export type CommandGroup = {
  label: string;
  actions: CommandAction[];
};

const registry = new Map<string, CommandGroup>();

export function getCommandGroups() {
  return [...registry.values()];
}

export function useCommandActions(
  label: string,
  actions: (CommandAction | false | null | undefined)[]
) {
  const id = useId();

  useEffect(() => {
    registry.set(id, {
      label,
      actions: actions.filter((action) => !!action),
    });

    return () => {
      registry.delete(id);
    };
  });
}
