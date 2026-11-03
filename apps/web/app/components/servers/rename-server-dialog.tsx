import { useMutation } from '@tanstack/react-query';
import { useId, useState } from 'react';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { Dialog } from '~/components/ui/dialog';
import { Input } from '~/components/ui/input';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { renameServerOptions, serverQueries } from '~/queries/servers';

type RenameServerDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  serverId: string;
  serverName: string;
};

export function RenameServerDialog(props: RenameServerDialogProps) {
  const { open, onOpenChange, serverId, serverName } = props;

  const nameId = useId();
  const [name, setName] = useState(serverName);

  const { mutate: renameServer, isPending } = useMutation({
    ...renameServerOptions(),
    meta: { errorMessage: 'The server could not be renamed' },
  });

  const trimmed = name.trim();
  const canSave = trimmed.length > 0 && trimmed !== serverName;

  const rename = () =>
    renameServer(
      { serverId, name: trimmed },
      {
        onSuccess: () => {
          queryClient.invalidateQueries({ queryKey: serverQueries.all() });
          toast.success(`Renamed to ${trimmed}`);
          onOpenChange(false);
        },
      }
    );

  return (
    <Dialog.Root
      open={open}
      onOpenChange={(nextOpen) => {
        if (nextOpen) {
          setName(serverName);
        }
        onOpenChange(nextOpen);
      }}
    >
      <Dialog.Portal>
        <Dialog.Backdrop />
        <Dialog.Popup>
          <Dialog.Header>
            <Dialog.Title>Rename Server</Dialog.Title>
          </Dialog.Header>

          <form
            onSubmit={(event) => {
              event.preventDefault();
              if (canSave) {
                rename();
              }
            }}
          >
            <Dialog.Body>
              <div className="flex flex-col gap-1.5">
                <label htmlFor={nameId} className="text-body font-medium">
                  Name
                </label>
                <Input
                  id={nameId}
                  value={name}
                  onValueChange={setName}
                  onFocus={(event) => event.currentTarget.select()}
                  maxLength={100}
                  required
                  autoFocus
                  autoComplete="off"
                  spellCheck={false}
                />
              </div>
            </Dialog.Body>

            <Dialog.Footer>
              <Dialog.Close
                render={<Button variant="secondary" className="w-fit" disabled={isPending} />}
              >
                Cancel
              </Dialog.Close>
              <Button type="submit" className="w-fit" disabled={isPending || !canSave}>
                {isPending && <Icon name="loader" className="animate-spin" />}
                Save
              </Button>
            </Dialog.Footer>
          </form>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
