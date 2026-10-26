import type { Hotkey } from '@tanstack/react-hotkeys';
import type { ReactElement } from 'react';

import { Tooltip } from '~/components/ui/tooltip';
import { formatHotkey } from '~/utils/hotkey';

type SidebarTooltipProps = {
  label: string;
  shortcut?: Hotkey;
  side?: 'right' | 'bottom';
  disabled?: boolean;
  trigger: ReactElement<Record<string, unknown>>;
};

export function SidebarTooltip(props: SidebarTooltipProps) {
  const { label, shortcut, side = 'right', disabled, trigger } = props;

  return (
    <Tooltip.Root disabled={disabled}>
      <Tooltip.Trigger render={trigger} />
      <Tooltip.Portal>
        <Tooltip.Positioner side={side} sideOffset={side === 'right' ? 14 : 6}>
          <Tooltip.Popup>
            {label}
            {shortcut && (
              <Tooltip.Shortcut>{formatHotkey(shortcut)}</Tooltip.Shortcut>
            )}
          </Tooltip.Popup>
        </Tooltip.Positioner>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}
