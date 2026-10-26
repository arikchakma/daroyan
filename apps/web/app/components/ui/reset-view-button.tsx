import { Icon } from '~/components/icons/icon';
import { Tooltip } from '~/components/ui/tooltip';

type ResetViewButtonProps = {
  onReset: () => void;
};

export function ResetViewButton(props: ResetViewButtonProps) {
  const { onReset } = props;

  return (
    <Tooltip.Root>
      <Tooltip.Trigger
        delay={200}
        aria-label="Reset View"
        onClick={onReset}
        className="flex size-7 shrink-0 items-center justify-center rounded-full outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40"
      >
        <Icon name="repeat" />
      </Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Positioner side="bottom">
          <Tooltip.Popup>
            <Tooltip.Arrow />
            Reset View
          </Tooltip.Popup>
        </Tooltip.Positioner>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}
