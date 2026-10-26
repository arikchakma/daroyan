import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { finishSlackInstallOptions, slackInstallOptions } from '~/queries/alerts';
import { CONNECTED_MESSAGE } from '~/utils/alerts';

export function meta() {
  return [{ title: 'Pick a Slack Channel · Daroyan' }];
}

export default function SlackChannelPage() {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const installId = searchParams.get('install') ?? '';
  const [channelId, setChannelId] = useState<string>();
  const [search, setSearch] = useState('');

  const { data: install, error } = useQuery({ ...slackInstallOptions(installId), enabled: !!installId });
  const finish = useMutation({
    ...finishSlackInstallOptions(),
    meta: { isSilent: true },
    onSuccess: (integration) => {
      if (window.opener) {
        window.opener.postMessage(
          { type: CONNECTED_MESSAGE, integrationId: integration.id },
          window.location.origin
        );
        window.close();
        return;
      }
      navigate(`/settings/integrations?connected=${integration.id}`, { replace: true });
    },
  });

  const term = search.trim().toLowerCase();
  const channels = (install?.channels ?? []).filter((channel) => channel.name.toLowerCase().includes(term));
  const failure = error?.message ?? finish.error?.message;

  return (
    <div className="flex min-h-dvh justify-center bg-surface-page p-6">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (channelId) {
            finish.mutate({ installId, channelId });
          }
        }}
        className="flex w-110 max-w-full flex-col gap-4"
      >
        <div className="flex items-center gap-3">
          <span className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-surface-fill">
            <Icon name="logo-slack" className="size-5" />
          </span>
          <div className="flex min-w-0 flex-col gap-0.5">
            <h1 className="text-title-section font-medium">Pick a Channel</h1>
            <p className="text-body text-text-secondary">
              {install ? `Daroyan posts alerts here in ${install.team_name}.` : 'Loading your Slack channels…'}
            </p>
          </div>
        </div>

        {install && (
          <>
            <label className="flex h-9 items-center gap-2 rounded-lg border border-border-ring bg-surface-default px-3 focus-within:border-border-strong">
              <Icon name="search" className="text-icon-secondary" />
              <input
                type="search"
                aria-label="Search Channels"
                placeholder="Search channels"
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                className="min-w-0 flex-1 bg-transparent text-body outline-none placeholder:text-text-tertiary"
              />
            </label>
            <div
              role="radiogroup"
              aria-label="Channel"
              className="flex max-h-90 flex-col overflow-y-auto rounded-2xl border border-border-ring bg-surface-default [&>*+*]:border-t [&>*+*]:border-border-row"
            >
              {channels.map((channel) => (
                <button
                  key={channel.id}
                  type="button"
                  role="radio"
                  aria-checked={channelId === channel.id}
                  onClick={() => setChannelId(channel.id)}
                  className="flex items-center gap-2.5 px-3.5 py-2.5 text-left text-body outline-none hover:bg-surface-subtle focus-visible:bg-surface-subtle aria-checked:bg-surface-subtle"
                >
                  <Icon name="hash" className="text-icon-secondary" />
                  <span className="min-w-0 flex-1 truncate">{channel.name}</span>
                  {channelId === channel.id && <Icon name="check" />}
                </button>
              ))}
              {channels.length === 0 && (
                <p className="px-3.5 py-3 text-body text-text-secondary">No public channel matches.</p>
              )}
            </div>
          </>
        )}

        {failure && (
          <p role="alert" className="flex items-start gap-2 rounded-lg bg-tone-critical-bg px-3 py-2 text-body text-tone-critical-fg">
            <Icon name="triangle-alert" className="mt-0.5" />
            {failure}
          </p>
        )}

        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" className="w-fit" onClick={() => window.close()}>
            Cancel
          </Button>
          <Button type="submit" className="w-fit" disabled={!channelId || finish.isPending}>
            {finish.isPending && <Icon name="loader" className="animate-spin" />}
            Post Alerts Here
          </Button>
        </div>
      </form>
    </div>
  );
}
