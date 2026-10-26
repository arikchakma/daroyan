import { useHotkey } from '@tanstack/react-hotkeys';

import { useUnsavedChanges } from '~/hooks/use-unsaved-changes';
import { cn } from '~/utils/classname';

type SaveBarProps = {
  isDirty: boolean;
  isSaving?: boolean;
  canSave?: boolean;
  onSave: () => void;
  onDiscard: () => void;
};

export function SaveBar(props: SaveBarProps) {
  const {
    isDirty,
    isSaving = false,
    canSave = true,
    onSave,
    onDiscard,
  } = props;

  const { isHighlighted } = useUnsavedChanges({ isDirty, isSaving });
  const isSaveDisabled = !canSave || isSaving;

  useHotkey(
    'Mod+S',
    () => {
      if (!isSaveDisabled) {
        onSave();
      }
    },
    { enabled: isDirty }
  );

  if (!isDirty) {
    return null;
  }

  return (
    <div className="pointer-events-none absolute top-4 right-1 left-0 z-30 flex justify-center">
      <div
        role="region"
        aria-label="Unsaved Changes"
        data-highlighted={isHighlighted || undefined}
        className={cn(
          'pointer-events-auto flex items-center gap-1 rounded-full bg-sidebar-bg py-1 pr-1 pl-3.5 shadow-float ring-1 ring-border-elevated transition-shadow ring-inset',
          isHighlighted && 'ring-2 ring-white motion-safe:animate-shake'
        )}
      >
        <span className="mr-2 text-body font-medium whitespace-nowrap text-sidebar-text">
          Unsaved changes
        </span>
        <button
          type="button"
          disabled={isSaving}
          onClick={onDiscard}
          className="flex h-7 items-center rounded-full bg-sidebar-search px-3 text-body font-medium text-sidebar-text outline-none hover:bg-sidebar-item-active focus-visible:ring-2 focus-visible:ring-white/40 disabled:opacity-50"
        >
          Discard
        </button>
        <button
          type="button"
          disabled={isSaveDisabled}
          onClick={onSave}
          className="light flex h-7 items-center rounded-full bg-surface-default px-3 text-body font-medium text-text-primary outline-none hover:bg-surface-default/90 focus-visible:ring-2 focus-visible:ring-white/40 disabled:opacity-70"
        >
          {isSaving ? 'Saving…' : 'Save'}
        </button>
      </div>
    </div>
  );
}
