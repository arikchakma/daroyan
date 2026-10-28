import { Icon } from '~/components/icons/icon';
import { Code } from '~/components/ui/code';
import { useCopyToClipboard } from '~/hooks/use-copy-to-clipboard';
import type { CodeLanguage } from '~/lib/highlighter';

type CodeBlockProps = {
  label: string;
  code: string;
  language: CodeLanguage;
};

export function CodeBlock(props: CodeBlockProps) {
  const { label, code, language } = props;

  const [copy, isCopied] = useCopyToClipboard();

  return (
    <div className="group relative overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card">
      <Code code={code} language={language} className="py-3.5 pr-12 pl-4 text-code" />
      <button
        type="button"
        aria-label={`Copy ${label}`}
        onClick={() => void copy(code)}
        className="absolute top-2 right-2 flex size-7 items-center justify-center rounded-full transition-opacity outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 pointer-fine:opacity-0 pointer-fine:group-focus-within:opacity-100 pointer-fine:group-hover:opacity-100"
      >
        <Icon name={isCopied ? 'check' : 'copy'} />
      </button>
    </div>
  );
}
