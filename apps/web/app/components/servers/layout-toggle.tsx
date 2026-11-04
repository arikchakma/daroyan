import { Toggle } from '@base-ui/react/toggle';
import { ToggleGroup } from '@base-ui/react/toggle-group';

import { Icon } from '~/components/icons/icon';
import type { ServersLayout } from '~/utils/servers';

type LayoutToggleProps = {
  layout: ServersLayout;
  onLayoutChange: (layout: ServersLayout) => void;
};

export function LayoutToggle(props: LayoutToggleProps) {
  const { layout, onLayoutChange } = props;

  return (
    <ToggleGroup
      aria-label="Layout"
      value={[layout]}
      onValueChange={(value: ServersLayout[]) => value[0] && onLayoutChange(value[0])}
      className="flex shrink-0 items-center gap-0.5"
    >
      <Toggle
        value="list"
        aria-label="List"
        className="flex size-7 items-center justify-center rounded-sm text-text-tertiary outline-none hover:bg-surface-fill hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40 data-pressed:bg-surface-fill data-pressed:text-icon-default"
      >
        <Icon name="list" />
      </Toggle>
      <Toggle
        value="grid"
        aria-label="Grid"
        className="flex size-7 items-center justify-center rounded-sm text-text-tertiary outline-none hover:bg-surface-fill hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40 data-pressed:bg-surface-fill data-pressed:text-icon-default"
      >
        <Icon name="layout-grid" />
      </Toggle>
    </ToggleGroup>
  );
}
