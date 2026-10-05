import { cn } from "@/lib/utils";
import type { StrengthResult } from "@/lib/crypto";

interface StrengthMeterProps {
  strength: StrengthResult | null;
  className?: string;
  showDetails?: boolean;
}

export function StrengthMeter({ strength, className, showDetails = true }: StrengthMeterProps) {
  if (!strength) return null;

  const percent = ((strength.score + 1) / 6) * 100;

  return (
    <div className={cn("space-y-2", className)}>
      {/* Bar */}
      <div className="flex items-center gap-3">
        <div className="bg-muted h-2 flex-1 overflow-hidden rounded-full">
          <div
            className="h-full rounded-full transition-all duration-500 ease-out"
            style={{
              width: `${percent}%`,
              background: `linear-gradient(90deg, ${strength.color}88, ${strength.color})`,
            }}
          />
        </div>
        <span className="text-xs font-semibold tabular-nums" style={{ color: strength.color }}>
          {strength.label}
        </span>
      </div>

      {showDetails && (
        <div className="text-muted-foreground flex items-center justify-between text-[11px]">
          <span>{strength.feedback}</span>
          <span className="tabular-nums">Crack time: {strength.crackTimeDisplay}</span>
        </div>
      )}
    </div>
  );
}

/** Color-coded score badge for compact display */
export function StrengthBadge({ score, label }: { score: number; label: string }) {
  const colors = ["#DC2626", "#EF4444", "#F59E0B", "#10B981", "#2563EB", "#7C3AED"];
  const color = colors[score] ?? colors[4];

  return (
    <span
      className="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold"
      style={{ backgroundColor: `${color}18`, color }}
    >
      <span className="size-1.5 rounded-full" style={{ backgroundColor: color }} />
      {label}
    </span>
  );
}
