import { Icon } from '~/components/icons/icon';
import { Menu } from '~/components/ui/menu';
import type { SavedLogView } from '~/utils/log-views';

type SavedViewItemProps = {
  view: SavedLogView;
  isActive: boolean;
  onApply: (view: SavedLogView) => void;
  onRemove: (view: SavedLogView) => void;
};

export function SavedViewItem(props: SavedViewItemProps) {
  const { view, isActive, onApply, onRemove } = props;

  return (
    <Menu.Item className="group" onClick={() => onApply(view)}>
      <span className="min-w-0 flex-1 truncate">{view.name}</span>
      {isActive && <Icon name="check" />}
      <button
        type="button"
        aria-label={`Remove ${view.name}`}
        onClick={(event) => {
          event.stopPropagation();
          onRemove(view);
        }}
        className="-mr-1 hidden size-6 shrink-0 items-center justify-center rounded-sm text-text-tertiary outline-none group-data-highlighted:flex hover:text-text-primary"
      >
        <Icon name="trash" />
      </button>
    </Menu.Item>
  );
}
