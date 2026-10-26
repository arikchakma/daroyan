import { Dialog } from '~/components/ui/dialog';
import { Kbd } from '~/components/ui/kbd';
import { formatHotkey } from '~/utils/hotkey';
import { NAV_ITEMS } from '~/utils/navigation';

type ShortcutsDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export function ShortcutsDialog(props: ShortcutsDialogProps) {
  const { open, onOpenChange } = props;

  const sections = [
    {
      label: 'General',
      shortcuts: [
        { label: 'Open Command Menu', keys: [formatHotkey('Mod+K')] },
        { label: 'Toggle Sidebar', keys: [formatHotkey('Mod+B')] },
        { label: 'Save Changes', keys: [formatHotkey('Mod+S')] },
        { label: 'Keyboard Shortcuts', keys: ['?'] },
      ],
    },
    {
      label: 'Go To',
      shortcuts: NAV_ITEMS.flatMap((item) =>
        item.shortcut ? [{ label: item.label, keys: ['G', item.shortcut] }] : []
      ),
    },
  ];

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Backdrop />
        <Dialog.Popup>
          <Dialog.Header>
            <Dialog.Title>Keyboard Shortcuts</Dialog.Title>
          </Dialog.Header>
          <Dialog.Body className="max-h-[min(70dvh,36rem)] scroll-fade-y scrollbar-none gap-4 overflow-y-auto">
            {sections.map((section) => (
              <section key={section.label} className="flex flex-col">
                <h3 className="pb-1 text-caption font-medium text-text-tertiary">
                  {section.label}
                </h3>
                {section.shortcuts.map((shortcut) => (
                  <div
                    key={shortcut.label}
                    className="flex h-8 items-center justify-between gap-3 text-body text-text-primary"
                  >
                    {shortcut.label}
                    <span className="flex gap-1">
                      {shortcut.keys.map((key) => (
                        <Kbd key={key}>{key}</Kbd>
                      ))}
                    </span>
                  </div>
                ))}
              </section>
            ))}
          </Dialog.Body>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
