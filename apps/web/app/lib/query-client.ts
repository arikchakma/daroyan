import { MutationCache, QueryClient } from '@tanstack/react-query';

import { ApiError } from './http';
import { getToastError } from './toast';

declare module '@tanstack/react-query' {
  interface Register {
    mutationMeta: {
      errorMessage?: string;
      isSilent?: boolean;
    };
  }
}

export const queryClient = new QueryClient({
  mutationCache: new MutationCache({
    onError: (error, _variables, _context, mutation) => {
      if (mutation.meta?.isSilent) {
        return;
      }
      getToastError({
        error,
        fallback: mutation.meta?.errorMessage ?? 'Something went wrong',
      });
    },
  }),
  defaultOptions: {
    queries: {
      staleTime: 5_000,
      retry: (failureCount, error) => {
        if (error instanceof ApiError && error.status < 500) {
          return false;
        }
        return failureCount < 2;
      },
      refetchOnReconnect: false,
    },
  },
});
