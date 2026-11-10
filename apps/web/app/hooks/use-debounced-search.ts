import { useEffect, useEffectEvent, useState } from 'react';

export const SEARCH_DEBOUNCE_MS = 300;

export function useDebouncedSearch(search: string, onSearchChange: (search: string) => void) {
  const [text, setText] = useState(search);
  const [sent, setSent] = useState(search);
  const [seen, setSeen] = useState(search);

  if (search !== seen) {
    setSeen(search);
    if (search !== sent) {
      setText(search);
      setSent(search);
    }
  }

  const send = useEffectEvent((next: string) => {
    setSent(next);
    onSearchChange(next);
  });

  useEffect(() => {
    if (text === sent) {
      return;
    }

    const timer = setTimeout(() => send(text), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [text, sent]);

  return [text, setText] as const;
}
