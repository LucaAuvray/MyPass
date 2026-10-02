import { useState, useRef, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Check, Copy } from "lucide-react";
import { cn } from "@/lib/utils";
import { useClipboard } from "@/hooks/useClipboard";

interface CopyButtonProps {
  text: string;
  className?: string;
  label?: string;
}

export function CopyButton({ text, className, label }: CopyButtonProps) {
  const { t } = useTranslation();
  const { copy } = useClipboard({});
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => { if (timerRef.current) clearTimeout(timerRef.current); };
  }, []);

  const handleCopy = async () => {
    const ok = await copy(text);
    if (ok) {
      setCopied(true);
      timerRef.current = setTimeout(() => setCopied(false), 2000);
    }
  };

  return (
    <button
      onClick={handleCopy}
      className={cn(
        "inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs font-medium transition-colors",
        "hover:bg-secondary",
        copied ? "text-green-600" : "text-muted-foreground hover:text-foreground",
        className,
      )}
      title={label ?? t("entries.copy")}
    >
      {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
      {label && <span>{copied ? t("entries.copied") : label}</span>}
    </button>
  );
}
