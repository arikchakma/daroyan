import { detectPlatform, formatForDisplay } from '@tanstack/react-hotkeys';
import type { Hotkey } from '@tanstack/react-hotkeys';

export function formatHotkey(hotkey: Hotkey) {
  return formatForDisplay(hotkey, {
    separatorToken: detectPlatform() === 'mac' ? '' : null,
  });
}
