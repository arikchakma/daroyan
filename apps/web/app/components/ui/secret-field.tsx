import { Icon } from "~/components/icons/icon";
import { useCopyToClipboard } from "~/hooks/use-copy-to-clipboard";
import { cn } from "~/utils/classname";

type SecretFieldProps = {
  label: string;
  value: string;
  variant?: "card" | "flat";
};

export function SecretField(props: SecretFieldProps) {
  const { label, value, variant = "card" } = props;

  const [copy, isCopied] = useCopyToClipboard();

  return (
    <div
      role="group"
      aria-label={label}
      className={cn(
        "flex h-13 items-center gap-1 rounded-2xl border border-border-ring bg-surface-default pr-2.5 pl-4 shadow-card",
        variant === "flat" &&
          "rounded-lg border-0 bg-surface-fill pr-2 shadow-none",
      )}
    >
      <span className="min-w-0 flex-1 truncate font-mono text-code select-all">
        {value}
      </span>
      <button
        type="button"
        aria-label={isCopied ? "Copied" : `Copy ${label}`}
        onClick={() => void copy(value)}
        className="flex size-8 shrink-0 items-center justify-center rounded-full outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40"
      >
        <Icon name={isCopied ? "check" : "copy"} />
      </button>
    </div>
  );
}
