import { Fragment, Suspense, use } from 'react';

import { CODE_THEME, getHighlighter } from '~/lib/highlighter';
import type { CodeLanguage } from '~/lib/highlighter';
import { cn } from '~/utils/classname';

type CodeProps = {
  code: string;
  language: CodeLanguage;
  className?: string;
};

export function Code(props: CodeProps) {
  const { code, language, className } = props;

  return (
    <pre
      className={cn(
        'code-tokens font-mono wrap-anywhere whitespace-pre-wrap',
        className
      )}
    >
      <Suspense fallback={code}>
        <CodeTokens code={code} language={language} />
      </Suspense>
    </pre>
  );
}

type CodeTokensProps = {
  code: string;
  language: CodeLanguage;
};

function CodeTokens(props: CodeTokensProps) {
  const { code, language } = props;

  const highlighter = use(getHighlighter());
  const lines = highlighter.codeToTokensBase(code, {
    lang: language,
    theme: CODE_THEME,
  });

  return lines.map((line, lineIndex) => (
    <Fragment key={lineIndex}>
      {lineIndex > 0 && '\n'}
      {line.map((token, tokenIndex) => (
        <span key={tokenIndex} style={{ color: token.color }}>
          {token.content}
        </span>
      ))}
    </Fragment>
  ));
}
