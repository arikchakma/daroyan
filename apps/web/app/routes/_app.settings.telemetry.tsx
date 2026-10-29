import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";

import { Icon } from "~/components/icons/icon";
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
import { ConfirmDialog } from "~/components/ui/confirm-dialog";
import { useCopyToClipboard } from "~/hooks/use-copy-to-clipboard";
import { useIsAdmin } from "~/hooks/use-is-admin";
import { queryClient } from "~/lib/query-client";
import { toast } from "~/lib/toast";
import {
  createIngestTokenOptions,
  deleteIngestTokenOptions,
  listIngestTokensOptions,
} from "~/queries/traces";

export function meta() {
  return [{ title: "Telemetry · Daroyan" }];
}

export default function TelemetryPage() {
  const isAdmin = useIsAdmin();
  const { data, isLoading } = useQuery(listIngestTokensOptions());
  const tokens = data?.map((token) => ({
    ...token,
    detail:
      token.dropped_log_records > 0
        ? `${token.dropped_log_records.toLocaleString()} log records dropped over the budget`
        : undefined,
  }));
  const [isAdding, setIsAdding] = useState(false);
  const [revoking, setRevoking] = useState<TokenItem>();
  const [copy] = useCopyToClipboard();
  const endpoint = `${window.location.origin}/v1/otlp`;

  const revoke = useMutation({
    ...deleteIngestTokenOptions(),
    meta: { errorMessage: "The token could not be revoked" },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["ingest-tokens"] });
      toast.success(`Revoked ${revoking?.name ?? "the token"}`);
      setRevoking(undefined);
    },
  });

  return (
    <SettingsPage title="Telemetry" icon="layers">
      <SettingsBlock
        title="Ingest Tokens"
        description="OpenTelemetry SDKs send traces and logs with one of these tokens. Revoke a token to stop accepting data from it."
      >
        <TokenCard
          tokens={tokens}
          isLoading={isLoading}
          prefix="dy_ingest_"
          emptyTitle="No ingest tokens"
          emptyDescription="Create one, then paste it into your OpenTelemetry SDK settings."
          onAdd={isAdmin ? () => setIsAdding(true) : undefined}
          onRevoke={isAdmin ? setRevoking : undefined}
        />
      </SettingsBlock>

      <SettingsBlock
        title="Setup Guide"
        description="Send traces and logs from Node.js, Python, Go or Rust. Every SDK exports to this endpoint."
      >
        <SettingsCard className="[&>*+*]:border-t [&>*+*]:border-border-row">
          <div className="flex items-center gap-3 py-2.5 pr-2.5 pl-4">
            <IconTile icon="terminal" />
            <div className="flex min-w-0 flex-1 flex-col gap-px text-body">
              <span className="truncate font-medium">Set Up Traces</span>
              <span className="truncate text-text-secondary">
                Install the SDK and send your first span
              </span>
            </div>
            <Button
              nativeButton={false}
              variant="secondary"
              render={<Link to="/traces/setup" />}
              className="w-fit"
            >
              Open Guide
            </Button>
          </div>
          <div className="flex items-center gap-3 py-2.5 pr-2.5 pl-4">
            <IconTile icon="globe" />
            <div className="flex min-w-0 flex-1 flex-col gap-px text-body">
              <span className="truncate font-medium">OTLP Endpoint</span>
              <span className="truncate font-mono text-code text-text-secondary">
                {endpoint}
              </span>
            </div>
            <button
              type="button"
              aria-label="Copy OTLP endpoint"
              onClick={() =>
                void copy(endpoint).then(
                  (isCopied) =>
                    isCopied && toast.success("Copied the endpoint"),
                )
              }
              className="flex size-8 shrink-0 items-center justify-center rounded-full outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40"
            >
              <Icon name="copy" />
            </button>
          </div>
        </SettingsCard>
      </SettingsBlock>

      <AddTokenDialog
        open={isAdding}
        onOpenChange={setIsAdding}
        queryKey={["ingest-tokens"]}
        mutation={createIngestTokenOptions()}
        placeholder="production-api"
        nameHelp="Name it after the service or environment that sends with it."
        renderCreated={() => (
          <p className="text-small text-text-secondary">
            Send it as{" "}
            <span className="font-mono">
              authorization=Bearer &lt;token&gt;
            </span>{" "}
            in OTEL_EXPORTER_OTLP_HEADERS.
          </p>
        )}
      />
      <ConfirmDialog
        open={!!revoking}
        onOpenChange={(open) => !open && setRevoking(undefined)}
        title={`Revoke ${revoking?.name ?? "Token"}?`}
        description={
          <>
            SDKs using{" "}
            <span className="font-mono">dy_ingest_…{revoking?.hint}</span> stop
            sending traces and logs right away. Data already received is kept.
          </>
        }
        confirmLabel="Revoke Token"
        isPending={revoke.isPending}
        onConfirm={() => revoking && revoke.mutate({ tokenId: revoking.id })}
      />
    </SettingsPage>
  );
}
