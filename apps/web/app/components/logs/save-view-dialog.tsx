import { useId, useState } from 'react';

import { FilterChip } from '~/components/search/filter-chip';
import { Button } from '~/components/ui/button';
import { Dialog } from '~/components/ui/dialog';
import { Input } from '~/components/ui/input';
import type { ViewChip } from '~/utils/log-views';

type SaveViewDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  chips: ViewChip[];
  onSave: (name: string) => void;
};

export function SaveViewDialog(props: SaveViewDialogProps) {
  const { open, onOpenChange, chips, onSave } = props;

  const nameId = useId();
  const [name, setName] = useState('');

  const trimmed = name.trim();

  return (
    <Dialog.Root
      open={open}
      onOpenChange={(nextOpen) => {
        if (nextOpen) {
          setName('');
        }
        onOpenChange(nextOpen);
      }}
    >
      <Dialog.Portal>
        <Dialog.Backdrop />
        <Dialog.Popup>
          <Dialog.Header>
            <Dialog.Title>Save View</Dialog.Title>
          </Dialog.Header>

          <form
            onSubmit={(event) => {
              event.preventDefault();
              if (trimmed) {
                onSave(trimmed);
              }
            }}
          >
            <Dialog.Body>
              <Dialog.Description>
                Saves the level, filters, search and time range in this browser. Only you see it.
              </Dialog.Description>
              <div className="flex flex-col gap-1.5">
                <label htmlFor={nameId} className="text-body font-medium">
                  Name
                </label>
                <Input
                  id={nameId}
                  value={name}
                  onValueChange={setName}
                  placeholder="Roadmap API 5xx"
                  maxLength={100}
                  required
                  autoFocus
                  autoComplete="off"
                />
              </div>
              <div className="flex flex-col gap-1.5">
                <span className="text-body font-medium">Saves</span>
                <div className="flex flex-wrap gap-1">
                  {chips.map((chip) => (
                    <FilterChip key={chip.condition} condition={chip.condition} value={chip.value} />
                  ))}
                </div>
              </div>
            </Dialog.Body>

            <Dialog.Footer>
              <Dialog.Close render={<Button variant="secondary" className="w-fit" />}>Cancel</Dialog.Close>
              <Button type="submit" className="w-fit" disabled={!trimmed}>
                Save View
              </Button>
            </Dialog.Footer>
          </form>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
