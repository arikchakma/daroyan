import { useMutation } from "@tanstack/react-query";
import type { UseMutationOptions } from "@tanstack/react-query";
import { useId, useState } from "react";
import type { ReactNode } from "react";

import { Icon } from "~/components/icons/icon";
import type { IconName } from "~/components/icons/icon-paths";
import { SettingsCard } from "~/components/settings/settings-page";
import { Button } from "~/components/ui/button";
import { Dialog } from "~/components/ui/dialog";
import { Input } from "~/components/ui/input";
import { Menu } from "~/components/ui/menu";
import { useCopyToClipboard } from "~/hooks/use-copy-to-clipboard";
import { queryClient } from "~/lib/query-client";
import { toast } from "~/lib/toast";
import { formatDate, formatRelative } from "~/utils/format";

export type TokenItem = {
  id: string;
  name: string;
  hint: string;
  created_at: string;
  last_used_at?: string | null;
};

export type CreatedTokenItem = TokenItem & { token: string };

export function IconTile(props: { icon: IconName }) {
  const { icon } = props;

  return (
    <span className="flex size-7 shrink-0 items-center justify-center rounded-md bg-surface-fill">
      <Icon name={icon} />
    </span>
  );
}

type TokenCardProps = {
  tokens?: TokenItem[];
  isLoading: boolean;
  prefix: string;
  emptyTitle: string;
  emptyDescription: string;
  onAdd?: () => void;
  onRevoke?: (token: TokenItem) => void;
};

export function TokenCard(props: TokenCardProps) {
  const {
    tokens,
    isLoading,
    prefix,
    emptyTitle,
    emptyDescription,
    onAdd,
    onRevoke,
  } = props;

  return (
    <SettingsCard className="[&>*+*]:border-t [&>*+*]:border-border-row">
      {isLoading && (
        <p className="px-4 py-3 text-body text-text-secondary">Loading…</p>
      )}
      {tokens?.length === 0 && (
        <div className="flex flex-col items-center gap-0.5 px-4 py-7 text-center text-body">
          <span className="font-medium">{emptyTitle}</span>
          <span className="text-text-secondary">{emptyDescription}</span>
        </div>
      )}
      {tokens?.map((token) => (
        <TokenRow
          key={token.id}
          token={token}
          prefix={prefix}
          onRevoke={onRevoke && (() => onRevoke(token))}
        />
      ))}
      {onAdd && (
        <button
          type="button"
          onClick={onAdd}
          className="flex items-center gap-2.5 bg-surface-subtle px-4 py-2.5 text-left text-body font-medium outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 focus-visible:ring-inset"
        >
          <Icon name="plus" />
          Add Token
        </button>
      )}
    </SettingsCard>
  );
}

function TokenRow(props: {
  token: TokenItem;
  prefix: string;
  onRevoke?: () => void;
}) {
  const { token, prefix, onRevoke } = props;

  return (
    <div className="flex items-center gap-3 py-2.5 pr-2.5 pl-4 has-data-popup-open:bg-surface-row">
      <IconTile icon="key" />
      <div className="flex min-w-0 flex-1 flex-col gap-px text-body">
        <span className="truncate font-medium">{token.name}</span>
        <span className="truncate text-text-secondary">
          <span className="font-mono text-code">
            {prefix}…{token.hint}
          </span>{" "}
          · Created {formatDate(token.created_at)}
        </span>
      </div>
      <span className="shrink-0 text-body text-text-secondary">
        {token.last_used_at
          ? `Used ${formatRelative(token.last_used_at)}`
          : "Never used"}
      </span>
      {onRevoke && (
        <Menu.Root>
          <Menu.Trigger
            aria-label={`Actions for ${token.name}`}
            className="flex size-8 shrink-0 items-center justify-center rounded-full outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 data-popup-open:bg-surface-fill"
          >
            <Icon name="more" />
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Positioner align="end">
              <Menu.Popup className="min-w-40">
                <Menu.Item onClick={onRevoke} className="text-tone-critical-fg">
                  <Icon name="trash" />
                  Revoke Token
                </Menu.Item>
              </Menu.Popup>
            </Menu.Positioner>
          </Menu.Portal>
        </Menu.Root>
      )}
    </div>
  );
}

type AddTokenDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  queryKey: string[];
  mutation: UseMutationOptions<CreatedTokenItem, Error, { name: string }>;
  placeholder: string;
  nameHelp: string;
  renderCreated: (token: CreatedTokenItem) => ReactNode;
};

export function AddTokenDialog(props: AddTokenDialogProps) {
  const {
    open,
    onOpenChange,
    queryKey,
    mutation,
    placeholder,
    nameHelp,
    renderCreated,
  } = props;

  const nameId = useId();
  const [name, setName] = useState("");
  const [created, setCreated] = useState<CreatedTokenItem>();
  const [copy] = useCopyToClipboard();

  const create = useMutation({
    ...mutation,
    meta: { errorMessage: "The token could not be created" },
    onSuccess: (token) => {
      queryClient.invalidateQueries({ queryKey });
      setCreated(token);
    },
  });

  return (
    <Dialog.Root
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          setName("");
          setCreated(undefined);
        }
        onOpenChange(nextOpen);
      }}
    >
      <Dialog.Portal>
        <Dialog.Backdrop />
        <Dialog.Popup>
          {created ? (
            <>
              <Dialog.Header>
                <Dialog.Title>Token Created</Dialog.Title>
              </Dialog.Header>
              <Dialog.Body>
                <div className="flex flex-col gap-1.5">
                  <span className="text-body font-medium">{created.name}</span>
                  <div className="flex gap-2">
                    <Input
                      aria-label="Token"
                      value={created.token}
                      readOnly
                      onFocus={(event) => event.currentTarget.select()}
                      className="font-mono text-code"
                    />
                    <Button
                      type="button"
                      variant="secondary"
                      className="w-fit"
                      onClick={() =>
                        void copy(created.token).then(
                          (isCopied) =>
                            isCopied && toast.success("Copied the token"),
                        )
                      }
                    >
                      <Icon name="copy" />
                      Copy
                    </Button>
                  </div>
                  {renderCreated(created)}
                </div>
                <p className="flex items-center gap-2 rounded-lg bg-tone-attention-bg px-3 py-2 text-body text-tone-attention-fg">
                  <Icon name="triangle-alert" className="size-3.5 shrink-0" />
                  Copy it now. It won't be shown again.
                </p>
              </Dialog.Body>
              <Dialog.Footer>
                <Dialog.Close render={<Button className="w-fit" />}>
                  Done
                </Dialog.Close>
              </Dialog.Footer>
            </>
          ) : (
            <>
              <Dialog.Header>
                <Dialog.Title>Add Token</Dialog.Title>
              </Dialog.Header>
              <form
                onSubmit={(event) => {
                  event.preventDefault();
                  create.mutate({ name: name.trim() });
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
                      placeholder={placeholder}
                      maxLength={100}
                      required
                      autoFocus
                      autoComplete="off"
                    />
                    <p className="text-small text-text-secondary">{nameHelp}</p>
                  </div>
                </Dialog.Body>
                <Dialog.Footer>
                  <Dialog.Close
                    render={
                      <Button
                        variant="secondary"
                        className="w-fit"
                        disabled={create.isPending}
                      />
                    }
                  >
                    Cancel
                  </Dialog.Close>
                  <Button
                    type="submit"
                    className="w-fit"
                    disabled={create.isPending || !name.trim()}
                  >
                    {create.isPending && (
                      <Icon name="loader" className="animate-spin" />
                    )}
                    Create Token
                  </Button>
                </Dialog.Footer>
              </form>
            </>
          )}
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
