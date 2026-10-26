import { useId, useState } from 'react';

import { Icon } from '~/components/icons/icon';
import type { IconName } from '~/components/icons/icon-paths';

import { Button } from './button';
import { Dialog } from './dialog';
import { Input } from './input';

type ConfirmDialogProps = {
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  trigger?: React.ReactNode;
  tone?: 'default' | 'critical';
  title: string;
  description: React.ReactNode;
  details?: (React.ReactNode | ConfirmDetail)[];
  confirmWord?: string;
  confirmLabel: string;
  cancelLabel?: string;
  isPending: boolean;
  onConfirm: () => void;
};

type ConfirmDetail = { icon: IconName; label: React.ReactNode };

function isConfirmDetail(detail: unknown): detail is ConfirmDetail {
  return typeof detail === 'object' && detail !== null && 'icon' in detail;
}

export function ConfirmDialog(props: ConfirmDialogProps) {
  const {
    open,
    onOpenChange,
    trigger,
    tone = 'critical',
    title,
    description,
    details = [],
    confirmWord,
    confirmLabel,
    cancelLabel = 'Cancel',
    isPending,
    onConfirm,
  } = props;

  const confirmId = useId();
  const [confirmText, setConfirmText] = useState('');
  const isConfirmed = !confirmWord || confirmText === confirmWord;

  return (
    <Dialog.Root
      open={open}
      onOpenChange={(nextOpen) => {
        setConfirmText('');
        onOpenChange?.(nextOpen);
      }}
    >
      {trigger}
      <Dialog.Portal>
        <Dialog.Backdrop />
        <Dialog.Popup>
          <Dialog.Header>
            <Dialog.Title>{title}</Dialog.Title>
          </Dialog.Header>

          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (isConfirmed) {
                onConfirm();
              }
            }}
          >
            <Dialog.Body>
              <Dialog.Description className="text-balance">
                {description}
              </Dialog.Description>
              {details.length > 0 && (
                <ul className="flex flex-col gap-1.5 rounded-lg bg-surface-subtle px-3 py-2.5">
                  {details.map((detail, index) => (
                    <li
                      key={index}
                      className="flex items-start gap-2 text-body text-pretty [&_svg]:mt-0.75 [&_svg]:size-3.5 [&_svg]:shrink-0"
                    >
                      <Icon
                        name={isConfirmDetail(detail) ? detail.icon : 'info'}
                      />
                      <span>
                        {isConfirmDetail(detail) ? detail.label : detail}
                      </span>
                    </li>
                  ))}
                </ul>
              )}
              {confirmWord && (
                <div className="flex flex-col gap-1.5">
                  <label htmlFor={confirmId} className="text-body font-medium">
                    Type {confirmWord} to confirm
                  </label>
                  <Input
                    id={confirmId}
                    value={confirmText}
                    onValueChange={setConfirmText}
                    placeholder={confirmWord}
                    required
                    autoFocus
                    autoComplete="off"
                    spellCheck={false}
                  />
                </div>
              )}
            </Dialog.Body>

            <Dialog.Footer>
              <Dialog.Close
                render={
                  <Button
                    variant="secondary"
                    className="w-fit"
                    disabled={isPending}
                  />
                }
              >
                {cancelLabel}
              </Dialog.Close>
              <Button
                variant={tone === 'critical' ? 'critical' : 'primary'}
                className="w-fit"
                type="submit"
                disabled={isPending || !isConfirmed}
              >
                {isPending && <Icon name="loader" className="animate-spin" />}
                {confirmLabel}
              </Button>
            </Dialog.Footer>
          </form>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
