import { useState } from 'react';

import { readStorage, saveStorage } from '~/utils/storage';

export function useStoredState<T>(key: string, fallback: T) {
  const [value, setValue] = useState(() => readStorage(key, fallback));

  const setStoredValue = (next: T) => {
    setValue(next);
    saveStorage(key, next);
  };

  return [value, setStoredValue] as const;
}
