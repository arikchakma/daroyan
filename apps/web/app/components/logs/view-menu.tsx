import { useState } from 'react';

import { Icon } from '~/components/icons/icon';
import { SavedViewItem } from '~/components/logs/saved-view-item';
import { Menu } from '~/components/ui/menu';
import type { LogLevel } from '~/queries/logs';
import type { SavedLogView } from '~/utils/log-views';

export type LevelView = 'all' | LogLevel;

export const VIEWS: { value: LevelView; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'error', label: 'Errors' },
  { value: 'warn', label: 'Warnings and Errors' },
  { value: 'info', label: 'Info and Above' },
];

type ViewMenuProps = {
  view: LevelView;
  savedViews: SavedLogView[];
  activeView?: SavedLogView;
  canSaveView: boolean;
  onChange: (view: LevelView) => void;
  onApplyView: (view: SavedLogView) => void;
  onRemoveView: (view: SavedLogView) => void;
  onSaveView: () => void;
};

export function ViewMenu(props: ViewMenuProps) {
  const { view, savedViews, activeView, canSaveView, onChange, onApplyView, onRemoveView, onSaveView } = props;

  const [isOpen, setIsOpen] = useState(false);

  const current = VIEWS.find((item) => item.value === view) ?? VIEWS[0]!;
  const label = activeView?.name ?? current.label;

  return (
    <Menu.Root open={isOpen} onOpenChange={setIsOpen}>
      <Menu.Trigger
        aria-label={`View: ${label}`}
        className="flex h-7 max-w-56 shrink-0 items-center gap-1 rounded-sm pr-1.5 pl-2.5 text-body font-medium outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 data-popup-open:bg-surface-fill"
      >
        <span className="truncate">{label}</span>
        <Icon name="chevrons-updown" className="size-3.5 shrink-0" />
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Positioner align="start" sideOffset={6}>
          <Menu.Popup className="max-w-72">
            <Menu.Group>
              <Menu.GroupLabel>Levels</Menu.GroupLabel>
              <Menu.RadioGroup
                value={activeView ? '' : current.value}
                onValueChange={(value) => onChange(value as LevelView)}
              >
                {VIEWS.map((item) => (
                  <Menu.RadioItem key={item.value} value={item.value}>
                    {item.label}
                  </Menu.RadioItem>
                ))}
              </Menu.RadioGroup>
            </Menu.Group>
            {savedViews.length > 0 && (
              <>
                <Menu.Separator />
                <Menu.Group>
                  <Menu.GroupLabel>Saved Views</Menu.GroupLabel>
                  {savedViews.map((saved) => (
                    <SavedViewItem
                      key={saved.id}
                      view={saved}
                      isActive={saved.id === activeView?.id}
                      onApply={onApplyView}
                      onRemove={(removed) => {
                        setIsOpen(false);
                        onRemoveView(removed);
                      }}
                    />
                  ))}
                </Menu.Group>
              </>
            )}
            <Menu.Separator />
            <Menu.Item disabled={!canSaveView} onClick={onSaveView}>
              <Icon name="plus" />
              Save Current View…
            </Menu.Item>
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}
