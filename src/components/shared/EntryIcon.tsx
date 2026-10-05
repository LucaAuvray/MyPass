import { cn } from "@/lib/utils";
import { Lock } from "lucide-react";

interface EntryIconProps {
  url?: string;
  icon?: number | string;
  size?: "sm" | "md" | "lg";
  className?: string;
}

const sizeMap = {
  sm: "size-8 rounded-lg text-xs",
  md: "size-10 rounded-xl text-sm",
  lg: "size-12 rounded-2xl text-base",
};

const iconSizeMap = {
  sm: "size-4",
  md: "size-5",
  lg: "size-6",
};

export function EntryIcon({ url, size = "md", className }: EntryIconProps) {
  const hostname = url ? extractHostname(url) : null;

  return (
    <div
      className={cn(
        "from-primary/20 to-accent/20 text-primary flex shrink-0 items-center justify-center bg-gradient-to-br font-semibold",
        sizeMap[size],
        className,
      )}
    >
      {hostname ? (
        <span className={cn("tracking-wider uppercase", iconSizeMap[size])}>
          {hostname.slice(0, 2)}
        </span>
      ) : (
        <Lock className={iconSizeMap[size]} />
      )}
    </div>
  );
}

function extractHostname(url: string): string {
  try {
    return new URL(url).hostname.replace("www.", "");
  } catch {
    return url;
  }
}
