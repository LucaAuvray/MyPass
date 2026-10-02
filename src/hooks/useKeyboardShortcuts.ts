import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useAppStore } from "@/stores/appStore";

/**
 * Global keyboard shortcuts for MyPass.
 */
export function useKeyboardShortcuts() {
  const navigate = useNavigate();
  const setSearchOpen = useAppStore((s) => s.setSearchOpen);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;

      // ⌘K / Ctrl+K — Search
      if (e.key === "k" && mod) {
        e.preventDefault();
        setSearchOpen(true);
      }

      // ⌘P / Ctrl+P — Search (alt)
      if (e.key === "p" && mod) {
        e.preventDefault();
        setSearchOpen(true);
      }

      // ⌘N / Ctrl+N — New entry (handled by AppSidebar)
      // Navigation shortcuts
      if (mod && e.key === "1") { e.preventDefault(); navigate("/"); }
      if (mod && e.key === "3") { e.preventDefault(); navigate("/security"); }
      if (mod && e.key === "4") { e.preventDefault(); navigate("/browser"); }

      // Escape — close panels
      if (e.key === "Escape") {
        setSearchOpen(false);
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [navigate, setSearchOpen]);
}
