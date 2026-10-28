import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";

import {
  EXPIRES_ITEMS,
  InstallKeySelect,
  MAX_USES_ITEMS,
} from "~/components/servers/install-key-select";
import {
  SettingsBlock,
  SettingsCard,
  SettingsPage,
} from "~/components/settings/settings-page";
import {
  AddTokenDialog,
  IconTile,
  TokenCard,
} from "~/components/settings/token-card";
import type { TokenItem } from "~/components/settings/token-card";
import { Button } from "~/components/ui/button";
import { CodeBlock } from "~/components/ui/code-block";
import { ConfirmDialog } from "~/components/ui/confirm-dialog";
import { useIsAdmin } from "~/hooks/use-is-admin";
import { queryClient } from "~/lib/query-client";
import { toast } from "~/lib/toast";
import {
  createInstallTokenOptions,
  listInstallTokensOptions,
  revokeInstallTokenOptions,
} from "~/queries/install-tokens";
import type { InstallTokenLimits } from "~/queries/install-tokens";
import { formatDate, formatRelative } from "~/utils/format";

const DEFAULT_LIMITS: InstallTokenLimits = {
  expires_in_hours: null,
  max_uses: null,
};

type InstallToken = TokenItem & {
  expires_at?: string | null;
  max_uses?: number | null;
  uses: number;
};

function toTokenItem(token: InstallToken): TokenItem {
  const usage =
    token.max_uses === null || token.max_uses === undefined
      ? `${token.uses} ${token.uses === 1 ? "server" : "servers"}`
      : `${token.uses} of ${token.max_uses}`;

  return {
    ...token,
    detail: token.expires_at
      ? `Expires ${formatDate(token.expires_at)}`
      : "Never expires",
    activity: token.last_used_at
      ? `${usage} · ${formatRelative(token.last_used_at)}`
      : "Never used",
  };
}

export function meta() {
  return [{ title: "Agents · Daroyan" }];
}

export default function AgentsPage() {
  const isAdmin = useIsAdmin();
  const { data: tokens, isLoading } = useQuery(listInstallTokensOptions());
  const [isAdding, setIsAdding] = useState(false);
  const [limits, setLimits] = useState(DEFAULT_LIMITS);
  const [revoking, setRevoking] = useState<TokenItem>();
  const installScript = `${window.location.origin}/install.sh`;

  const revoke = useMutation({
    ...revokeInstallTokenOptions(),
    meta: { errorMessage: "The key could not be revoked" },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["install-tokens"] });
      toast.success(`Revoked ${revoking?.name ?? "the key"}`);
      setRevoking(undefined);
    },
  });

  return (
    <SettingsPage title="Agents" icon="server">
      <SettingsBlock
        title="Install Keys"
        description="A new server enrolls with one of these keys, then gets its own token. Revoking a key stops new servers, not enrolled ones."
      >
        <TokenCard
          tokens={tokens?.map(toTokenItem)}
          isLoading={isLoading}
          prefix="dy_agent_"
          noun="Key"
          emptyTitle="No install keys"
          emptyDescription="Create one to add servers from Ansible, cloud-init or a script."
          onAdd={isAdmin ? () => setIsAdding(true) : undefined}
          onRevoke={isAdmin ? setRevoking : undefined}
        />
      </SettingsBlock>

      <SettingsBlock
        title="Add a Server"
        description="Run the install command as root with any key above. The Add Server page makes a one-off key for you."
      >
        <SettingsCard className="[&>*+*]:border-t [&>*+*]:border-border-row">
          <div className="flex items-center gap-3 py-2.5 pr-2.5 pl-4">
            <IconTile icon="terminal" />
            <div className="flex min-w-0 flex-1 flex-col gap-px text-body">
              <span className="truncate font-medium">Add Server</span>
              <span className="truncate text-text-secondary">
                Copy a one-off install command
              </span>
            </div>
            <Button
              nativeButton={false}
              variant="secondary"
              render={<Link to="/servers/new" />}
              className="w-fit"
            >
              Add Server
            </Button>
          </div>
          <div className="flex items-center gap-3 py-2.5 pr-2.5 pl-4">
            <IconTile icon="globe" />
            <div className="flex min-w-0 flex-1 flex-col gap-px text-body">
              <span className="truncate font-medium">Install Script</span>
              <span className="truncate font-mono text-code text-text-secondary">
                {installScript}
              </span>
            </div>
          </div>
        </SettingsCard>
      </SettingsBlock>

      <AddTokenDialog
        open={isAdding}
        onOpenChange={(open) => {
          if (!open) {
            setLimits(DEFAULT_LIMITS);
          }
          setIsAdding(open);
        }}
        queryKey={["install-tokens"]}
        mutation={createInstallTokenOptions(limits)}
        noun="Key"
        placeholder="ansible-fleet"
        nameHelp="Name it after where it lives, like an Ansible role or a cloud-init template."
        fields={
          <>
            <div className="flex flex-col gap-1.5">
              <span className="text-body font-medium">Expires</span>
              <InstallKeySelect
                label="Expires"
                items={EXPIRES_ITEMS}
                value={limits.expires_in_hours}
                onValueChange={(expires_in_hours) =>
                  setLimits({ ...limits, expires_in_hours })
                }
              />
              <p className="text-small text-text-secondary">
                A key that never expires works until you revoke it.
              </p>
            </div>
            <div className="flex flex-col gap-1.5">
              <span className="text-body font-medium">Works for</span>
              <InstallKeySelect
                label="Works for"
                items={MAX_USES_ITEMS}
                value={limits.max_uses}
                onValueChange={(max_uses) => setLimits({ ...limits, max_uses })}
              />
              <p className="text-small text-text-secondary">
                Each server uses the key once, when it enrolls.
              </p>
            </div>
          </>
        }
        renderCreated={(created) => (
          <CodeBlock
            label="Install command"
            code={`curl -fsSL ${window.location.origin}/install.sh | \\\n  sudo DAROYAN_API_URL=${window.location.origin} DAROYAN_TOKEN=${created.token} sh`}
            language="shellscript"
          />
        )}
      />
      <ConfirmDialog
        open={!!revoking}
        onOpenChange={(open) => !open && setRevoking(undefined)}
        title={`Revoke ${revoking?.name ?? "Key"}?`}
        description={
          <>
            New servers can't enroll with{" "}
            <span className="font-mono">dy_agent_…{revoking?.hint}</span>{" "}
            anymore. Servers that already enrolled keep reporting.
          </>
        }
        confirmLabel="Revoke Key"
        isPending={revoke.isPending}
        onConfirm={() => revoking && revoke.mutate({ tokenId: revoking.id })}
      />
    </SettingsPage>
  );
}
