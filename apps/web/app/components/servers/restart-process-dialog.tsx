import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { ConfirmDialog } from '~/components/ui/confirm-dialog';
import { useLinkAction } from '~/hooks/use-link-action';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { listAlertsOptions } from '~/queries/alerts';
import { meOptions } from '~/queries/auth';
import { runCommandOptions } from '~/queries/servers';
import { formatRelative } from '~/utils/format';

type RestartProcessDialogProps = {
  server: Schemas['ServerDetail'];
};

export function RestartProcessDialog(props: RestartProcessDialogProps) {
  const { server } = props;

  const { data: me, isSuccess: isMeLoaded } = useQuery(meOptions());
  const { data: firing = [] } = useQuery(listAlertsOptions('firing'));

  const [name, setName] = useState('');
  const [isOpen, setIsOpen] = useState(false);

  const { mutate: runCommand, isPending } = useMutation({
    ...runCommandOptions(),
    meta: { errorMessage: `${name} could not be restarted` },
  });

  const processes = server.last_report?.processes ?? [];

  useLinkAction('restart', isMeLoaded, (processName) => {
    const process = processes.find((item) => item.name === processName);
    if (me?.role !== 'admin') {
      toast.error('Only admins can restart processes. Ask an admin.');
    } else if (!process) {
      toast.info(`${server.name} no longer watches ${processName}.`);
    } else if (!process.controllable) {
      toast.info(`Daroyan can’t restart ${processName}. It is watch only.`);
    } else if (process.status === 'running') {
      toast.success(`${processName} is running again.`);
    } else {
      setName(processName);
      setIsOpen(true);
    }
  });

  const alert = firing.find((item) => item.server_id === server.id && item.subject === name);

  const restart = () =>
    runCommand(
      { serverId: server.id, action: 'restart_process', name },
      {
        onSuccess: () => {
          toast.success(`Restarted ${name}`);
          setIsOpen(false);
          void queryClient.invalidateQueries({ queryKey: ['servers', server.id] });
        },
      }
    );

  return (
    <ConfirmDialog
      open={isOpen}
      onOpenChange={setIsOpen}
      tone="default"
      title={`Restart ${name}?`}
      description={`The agent on ${server.name} restarts it now. The alert resolves on its own once the process reports running again.`}
      details={[
        { icon: 'server', label: [server.name, ...server.labels].join(' · ') },
        ...(alert ? [{ icon: 'bell' as const, label: `${alert.rule_name} · ${formatRelative(alert.started_at)}` }] : []),
      ]}
      confirmLabel="Restart"
      isPending={isPending}
      onConfirm={restart}
    />
  );
}
