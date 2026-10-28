import { useQuery } from '@tanstack/react-query';

import { ServerBody } from '~/components/servers/server-body';
import { TopBar } from '~/components/servers/top-bar';
import { serverOptions } from '~/queries/servers';

import type { Route } from './+types/_app.servers.$serverId';

export default function ServerPage(props: Route.ComponentProps) {
  const { serverId } = props.params;

  const { data: server, error } = useQuery(serverOptions(serverId));

  if (error) {
    return (
      <div className="flex h-full flex-col bg-surface-page">
        <TopBar serverId={serverId} />
        <div className="flex flex-1 flex-col items-center justify-center gap-1 pb-20">
          <p className="text-title-section font-medium text-text-primary">Server Not Found</p>
          <p className="text-body text-text-secondary">It may have been removed.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-h-full flex-col bg-surface-page">
      <TopBar serverId={serverId} serverName={server?.name} />
      {server && <ServerBody server={server} />}
    </div>
  );
}
