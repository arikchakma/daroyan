import { useCallback, useEffect, useState } from 'react';

type CopyFn = (text: string) => Promise<boolean>;

export function useCopyToClipboard(): [CopyFn, boolean] {
  const [isCopied, setIsCopied] = useState(false);

  const copy: CopyFn = useCallback(async (text) => {
    if (!navigator?.clipboard) {
      console.warn('Clipboard not supported');
      return false;
    }

    try {
      await navigator.clipboard.writeText(text);
      setIsCopied(true);
      return true;
    } catch (error) {
      console.warn('Copy failed', error);
      return false;
    }
  }, []);

  useEffect(() => {
    if (!isCopied) {
      return;
    }

    const timeout = setTimeout(() => {
      setIsCopied(false);
    }, 2000);

    return () => clearTimeout(timeout);
  }, [isCopied]);

  return [copy, isCopied];
}
