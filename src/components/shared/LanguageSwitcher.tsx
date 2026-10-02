import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";

interface LanguageSwitcherProps {
  className?: string;
}

export function LanguageSwitcher({ className }: LanguageSwitcherProps) {
  const { i18n } = useTranslation();
  const currentLang = i18n.language?.startsWith("fr") ? "fr" : "en";

  const toggle = () => {
    i18n.changeLanguage(currentLang === "fr" ? "en" : "fr");
  };

  return (
    <button
      onClick={toggle}
      className={cn(
        "inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium transition-colors hover:bg-muted",
        className,
      )}
      title={currentLang === "fr" ? "Switch to English" : "Passer en français"}
    >
      <span className={currentLang === "fr" ? "opacity-100" : "opacity-40"}>🇫🇷</span>
      <span className="text-muted-foreground">/</span>
      <span className={currentLang === "en" ? "opacity-100" : "opacity-40"}>🇬🇧</span>
    </button>
  );
}
