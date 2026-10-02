import { Routes, Route, Navigate } from "react-router-dom";
import { useAppStore } from "./stores/appStore";
import AppShell from "./components/layout/AppShell";
import UnlockView from "./views/UnlockView";
import { AllItemsView } from "./views/AllItemsView";
import { SecurityView } from "./views/SecurityView";
import { PasskeysView } from "./views/PasskeysView";
import { BrowserIntegrationView } from "./views/BrowserIntegrationView";
import { SshAgentView } from "./views/SshAgentView";
import { SyncView } from "./views/SyncView";

function App() {
  const isLocked = useAppStore((s) => s.isLocked);

  if (isLocked) {
    return <UnlockView />;
  }

  return (
    <AppShell>
      <Routes>
        <Route path="/" element={<AllItemsView />} />
        <Route path="/security" element={<SecurityView />} />
        <Route path="/passkeys" element={<PasskeysView />} />
        <Route path="/browser" element={<BrowserIntegrationView />} />
        <Route path="/ssh-agent" element={<SshAgentView />} />
        <Route path="/sync" element={<SyncView />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </AppShell>
  );
}

export default App;
