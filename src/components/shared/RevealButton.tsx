import { useState } from "react";
import { Eye, EyeOff } from "lucide-react";
import { cn } from "@/lib/utils";

interface RevealButtonProps {
  className?: string;
}

export function useReveal() {
  const [revealed, setRevealed] = useState(false);
  return { revealed, toggle: () => setRevealed((r) => !r) };
}

export function RevealButton({ className }: RevealButtonProps) {
  const { revealed, toggle } = useReveal();
  return (
    <button
      onClick={toggle}
      className={cn(
        "rounded p-1 text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground",
        className,
      )}
      aria-label={revealed ? "Hide" : "Show"}
    >
      {revealed ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
    </button>
  );
}
