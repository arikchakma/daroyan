import type { Schemas } from '@daroyan/client';
import { useMutation } from '@tanstack/react-query';

import { Icon } from '~/components/icons/icon';
import { queryClient } from '~/lib/query-client';
import { serverQueries, starServerOptions } from '~/queries/servers';
import { cn } from '~/utils/classname';

type StarButtonProps = {
  server: Schemas['ServerItem'];
};

export function StarButton(props: StarButtonProps) {
  const { server } = props;

  const isStarred = !!server.starred_at;
  const listKey = serverQueries.list().queryKey;

  const star = useMutation({
    ...starServerOptions(),
    meta: { errorMessage: 'The star could not be saved' },
    onMutate: async (variables) => {
      await queryClient.cancelQueries({ queryKey: listKey });
      const previous = queryClient.getQueryData(listKey);
      const starredAt = variables.starred ? new Date().toISOString() : null;

      queryClient.setQueryData(listKey, (servers) =>
        servers?.map((item) => (item.id === variables.serverId ? { ...item, starred_at: starredAt } : item))
      );

      return { previous };
    },
    onError: (_error, _variables, context) => {
      queryClient.setQueryData(listKey, context?.previous);
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: serverQueries.all() }),
  });

  return (
    <button
      type="button"
      aria-label={isStarred ? `Unstar ${server.name}` : `Star ${server.name}`}
      aria-pressed={isStarred}
      onClick={() => star.mutate({ serverId: server.id, starred: !isStarred })}
      className={cn(
        'relative z-1 inline-flex size-6 items-center justify-center rounded-full outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40',
        isStarred ? 'text-star' : 'text-star-empty hover:text-text-tertiary'
      )}
    >
      <Icon name={isStarred ? 'star-fill' : 'star'} className="size-3.5 [--icon-glyph:currentColor]" />
    </button>
  );
}
