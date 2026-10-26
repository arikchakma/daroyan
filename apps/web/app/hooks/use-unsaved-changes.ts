import { useEffect, useState } from 'react';
import { useBeforeUnload, useBlocker } from 'react-router';

type UseUnsavedChangesOptions = {
  isDirty: boolean;
  isSaving: boolean;
};

export function useUnsavedChanges(options: UseUnsavedChangesOptions) {
  const { isDirty, isSaving } = options;

  const [blockedAt, setBlockedAt] = useState<number | null>(null);

  const blocker = useBlocker(
    ({ currentLocation, nextLocation, historyAction }) =>
      isDirty &&
      !isSaving &&
      historyAction !== 'REPLACE' &&
      currentLocation.pathname !== nextLocation.pathname
  );

  useBeforeUnload((event) => {
    if (isDirty) {
      event.preventDefault();
    }
  });

  useEffect(() => {
    if (blocker.state !== 'blocked') {
      return;
    }
    blocker.reset();
    setBlockedAt(Date.now());
  }, [blocker]);

  useEffect(() => {
    if (blockedAt === null) {
      return;
    }
    const timer = setTimeout(() => setBlockedAt(null), 1500);
    return () => clearTimeout(timer);
  }, [blockedAt]);

  return { isHighlighted: isDirty && blockedAt !== null };
}
