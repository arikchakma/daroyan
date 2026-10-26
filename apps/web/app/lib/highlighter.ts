import { createCssVariablesTheme, createHighlighterCore } from 'shiki/core';
import type { HighlighterCore } from 'shiki/core';
import { createJavaScriptRegexEngine } from 'shiki/engine/javascript';

export type CodeLanguage = 'shellscript' | 'yaml' | 'text';

export const CODE_THEME = 'daroyan';

const codeTheme = createCssVariablesTheme({
  name: CODE_THEME,
  variablePrefix: '--code-',
  variableDefaults: {},
  fontStyle: false,
});

let highlighter: Promise<HighlighterCore> | undefined;

export function getHighlighter() {
  highlighter ??= createHighlighterCore({
    themes: [codeTheme],
    langs: [import('shiki/langs/shellscript.mjs'), import('shiki/langs/yaml.mjs')],
    engine: createJavaScriptRegexEngine(),
  });

  return highlighter;
}
