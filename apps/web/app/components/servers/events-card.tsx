import { useQuery } from '@tanstack/react-query';

import { EventRow } from '~/components/servers/event-row';
import { Dialog } from '~/components/ui/dialog';
import { serverEventsOptions } from '~/queries/servers';

const VISIBLE_EVENTS = 4;

type EventsCardProps = {
  serverId: string;
  serverName: string;
};

export function EventsCard(props: EventsCardProps) {
  const { serverId, serverName } = props;

  const { data: events } = useQuery(serverEventsOptions(serverId));

  return (
    <div className="flex w-full shrink-0 flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card lg:w-85">
      <header className="flex items-center px-4 pt-2.5 pb-1">
        <h2 className="flex-1 text-caption font-medium text-text-primary">Events</h2>
        {events && events.length > 0 && (
          <Dialog.Root>
            <Dialog.Trigger className="rounded-xs text-body text-text-primary outline-none hover:underline focus-visible:underline">
              View All Events
            </Dialog.Trigger>
            <Dialog.Portal>
              <Dialog.Backdrop />
              <Dialog.Popup>
                <Dialog.Header>
                  <Dialog.Title>Events</Dialog.Title>
                  <Dialog.Description className="text-text-secondary">
                    The latest {events.length} events from {serverName}.
                  </Dialog.Description>
                </Dialog.Header>
                <ul className="max-h-[min(480px,70vh)] overflow-y-auto pb-2">
                  {events.map((event, index) => (
                    <EventRow key={`${event.at}-${index}`} event={event} />
                  ))}
                </ul>
              </Dialog.Popup>
            </Dialog.Portal>
          </Dialog.Root>
        )}
      </header>
      {events?.length === 0 && <p className="flex h-8.5 items-center px-4 text-body text-text-secondary">No events yet.</p>}
      <ul>
        {events?.slice(0, VISIBLE_EVENTS).map((event, index) => (
          <EventRow key={`${event.at}-${index}`} event={event} />
        ))}
      </ul>
    </div>
  );
}
