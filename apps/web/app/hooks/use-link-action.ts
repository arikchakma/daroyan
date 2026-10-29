import { useEffect, useEffectEvent } from 'react';
import { useSearchParams } from 'react-router';

export function useLinkAction(name: string, isReady: boolean, onAction: (value: string) => void) {
  const [searchParams, setSearchParams] = useSearchParams();
  const value = searchParams.get(name);
  const handle = useEffectEvent(onAction);

  useEffect(() => {
    if (!value || !isReady) {
      return;
    }

    handle(value);
    setSearchParams(
      (params) => {
        params.delete(name);
        return params;
      },
      { replace: true }
    );
  }, [name, value, isReady, setSearchParams]);
}
