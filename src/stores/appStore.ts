import { create } from "zustand";
import { persist } from "zustand/middleware";

interface AppState {
  isLocked: boolean;
  theme: "light" | "dark" | "system";
  sidebarCollapsed: boolean;
  searchOpen: boolean;
  /** Drawer de navigation mobile (sidebar en overlay) */
  mobileMenuOpen: boolean;
  /** Auto-lock delay in minutes; 0 = never */
  autoLockMinutes: number;

  lock: () => void;
  unlock: () => void;
  setTheme: (theme: "light" | "dark" | "system") => void;
  toggleSidebar: () => void;
  setSearchOpen: (open: boolean) => void;
  setMobileMenuOpen: (open: boolean) => void;
  setAutoLockMinutes: (minutes: number) => void;
}

export const useAppStore = create<AppState>()(
  persist(
    (set) => ({
      isLocked: true,
      theme: "system",
      sidebarCollapsed: false,
      searchOpen: false,
      mobileMenuOpen: false,
      autoLockMinutes: 15,

      lock: () => set({ isLocked: true }),
      unlock: () => set({ isLocked: false }),
      setTheme: (theme) => set({ theme }),
      toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
      setSearchOpen: (open) => set({ searchOpen: open }),
      setMobileMenuOpen: (open) => set({ mobileMenuOpen: open }),
      setAutoLockMinutes: (minutes) => set({ autoLockMinutes: minutes }),
    }),
    {
      name: "mypass-settings",
      partialize: (s) => ({ autoLockMinutes: s.autoLockMinutes }),
    },
  ),
);
