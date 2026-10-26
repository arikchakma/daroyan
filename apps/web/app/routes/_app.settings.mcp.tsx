import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { Icon } from "~/components/icons/icon";
import type { IconName } from "~/components/icons/icon-paths";
import {
  SettingsBlock,
  SettingsCard,
  SettingsPage,
} from "~/components/settings/settings-page";
import { AddTokenDialog, TokenCard } from "~/components/settings/token-card";
import type { TokenItem } from "~/components/settings/token-card";
import { CodeBlock } from "~/components/ui/code-block";
import { ConfirmDialog } from "~/components/ui/confirm-dialog";
import { SecretField } from "~/components/ui/secret-field";
import { Tabs } from "~/components/ui/tabs";
import { queryClient } from "~/lib/query-client";
import { toast } from "~/lib/toast";
import {
  createMcpTokenOptions,
  deleteMcpTokenOptions,
  listMcpTokensOptions,
} from "~/queries/mcp";

function getClaudeCommand(url: string, token: string) {
  return `claude mcp add --transport http daroyan ${url} \\\n  --header "Authorization: Bearer ${token}"`;
}

function getToolSetups(url: string) {
  const header = "Authorization: Bearer <token>";
  const headers = `"headers": { "Authorization": "Bearer <token>" }`;
  const replace = "Replace <token> with an MCP token from above.";

  return [
    {
      name: "Claude Code",
      icon: "logo-claude",
      intro: "Add the server from your terminal.",
      command: `claude mcp add --transport http daroyan ${url} --header "${header}"`,
      steps: [
        replace,
        "Run /mcp in Claude Code to check that daroyan is connected.",
      ],
    },
    {
      name: "Cursor",
      icon: "logo-cursor",
      intro: "Add the server to ~/.cursor/mcp.json.",
      command: `{ "mcpServers": { "daroyan": { "url": "${url}", ${headers} } } }`,
      steps: [replace, "Open Cursor Settings → MCP and turn on daroyan."],
    },
    {
      name: "VS Code",
      icon: "logo-vscode",
      intro: "Add the server from your terminal.",
      command: `code --add-mcp '{"name":"daroyan","type":"http","url":"${url}","headers":{"Authorization":"Bearer <token>"}}'`,
      steps: [replace, "Start the server from the MCP Servers list."],
    },
    {
      name: "Codex",
      icon: "logo-codex",
      intro: "Add the server from your terminal.",
      command: `codex mcp add daroyan --url ${url} --bearer-token-env-var DAROYAN_MCP_TOKEN`,
      steps: [
        "Set DAROYAN_MCP_TOKEN to an MCP token from above before you start Codex.",
      ],
    },
    {
      name: "Windsurf",
      icon: "logo-windsurf",
      intro: "Add the server to ~/.codeium/windsurf/mcp_config.json.",
      command: `{ "mcpServers": { "daroyan": { "serverUrl": "${url}", ${headers} } } }`,
      steps: [replace, "Refresh the MCP servers in Cascade."],
    },
  ] satisfies {
    name: string;
    icon: IconName;
    intro: string;
    command: string;
    steps: string[];
  }[];
}

export function meta() {
  return [{ title: "MCP · Daroyan" }];
}

export default function McpPage() {
  const { data: tokens, isLoading } = useQuery(listMcpTokensOptions());
  const [isAdding, setIsAdding] = useState(false);
  const [revoking, setRevoking] = useState<TokenItem>();
  const url = `${window.location.origin}/mcp`;
  const setups = getToolSetups(url);

  const revoke = useMutation({
    ...deleteMcpTokenOptions(),
    meta: { errorMessage: "The token could not be revoked" },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["mcp-tokens"] });
      toast.success(`Revoked ${revoking?.name ?? "the token"}`);
      setRevoking(undefined);
    },
  });

  return (
    <SettingsPage title="MCP" icon="bot">
      <SettingsBlock
        title="MCP Tokens"
        description="AI tools like Claude Code and Cursor use a token to read logs and traces, add checks and crons, and run commands as you. Revoke a token to cut off its tool."
      >
        <TokenCard
          tokens={tokens}
          isLoading={isLoading}
          prefix="dy_mcp_"
          emptyTitle="No MCP tokens"
          emptyDescription="Create one for each AI tool you connect."
          onAdd={() => setIsAdding(true)}
          onRevoke={setRevoking}
        />
      </SettingsBlock>

      <SettingsBlock
        title="Server URL"
        description="Paste this URL into any AI tool that supports MCP, with a token in the Authorization header."
      >
        <SecretField label="Server URL" value={url} />
      </SettingsBlock>

      <SettingsBlock title="Set Up a Tool">
        <SettingsCard>
          <Tabs.Root defaultValue={setups[0]!.name}>
            <Tabs.List aria-label="AI Tools">
              {setups.map((setup) => (
                <Tabs.Tab key={setup.name} value={setup.name}>
                  <Icon name={setup.icon} />
                  {setup.name}
                </Tabs.Tab>
              ))}
            </Tabs.List>
            {setups.map((setup) => (
              <Tabs.Panel
                key={setup.name}
                value={setup.name}
                className="flex flex-col gap-3 p-4 text-body"
              >
                <p>1. {setup.intro}</p>
                <SecretField
                  variant="flat"
                  label={`${setup.name} Setup Command`}
                  value={setup.command}
                />
                {setup.steps.map((step, index) => (
                  <p key={step}>
                    {index + 2}. {step}
                  </p>
                ))}
              </Tabs.Panel>
            ))}
          </Tabs.Root>
        </SettingsCard>
      </SettingsBlock>

      <AddTokenDialog
        open={isAdding}
        onOpenChange={setIsAdding}
        queryKey={["mcp-tokens"]}
        mutation={createMcpTokenOptions()}
        placeholder="Claude Code"
        nameHelp="Name it after the tool that uses it."
        renderCreated={(created) => (
          <div className="flex flex-col gap-1.5 pt-2">
            <span className="text-small text-text-secondary">
              Add it to Claude Code:
            </span>
            <CodeBlock
              label="Claude Code command"
              code={getClaudeCommand(url, created.token)}
              language="shellscript"
            />
          </div>
        )}
      />
      <ConfirmDialog
        open={!!revoking}
        onOpenChange={(open) => !open && setRevoking(undefined)}
        title={`Revoke ${revoking?.name ?? "Token"}?`}
        description={
          <>
            Tools using{" "}
            <span className="font-mono">dy_mcp_…{revoking?.hint}</span> lose
            access right away.
          </>
        }
        confirmLabel="Revoke Token"
        isPending={revoke.isPending}
        onConfirm={() => revoking && revoke.mutate({ tokenId: revoking.id })}
      />
    </SettingsPage>
  );
}
