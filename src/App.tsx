import { Routes, Route, Navigate } from "react-router-dom";
import { useAppStore } from "./stores/appStore";
import AppShell from "./components/layout/AppShell";
import UnlockView from "./views/UnlockView";
import { AllItemsView } from "./views/AllItemsView";
import { SecurityView } from "./views/SecurityView";
import { BrowserIntegrationView } from "./views/BrowserIntegrationView";
import { SshAgentView } from "./views/SshAgentView";
import { SyncView } from "./views/SyncView";
import { PasskeyRequestDialog } from "./components/passkeys/PasskeyRequestDialog";

function App() {
  const isLocked = useAppStore((s) => s.isLocked);

  return (
    <>
      {isLocked ? (
        <UnlockView />
      ) : (
        <AppShell>
          <Routes>
            <Route path="/" element={<AllItemsView />} />
            <Route path="/security" element={<SecurityView />} />
            <Route path="/browser" element={<BrowserIntegrationView />} />
            <Route path="/ssh-agent" element={<SshAgentView />} />
            <Route path="/sync" element={<SyncView />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </AppShell>
      )}
      {/* Outside AppShell: a passkey request sent right after unlock (the extension
          waits for it) must not arrive before the dialog listens. */}
      <PasskeyRequestDialog />
    </>
  );
}

export default App;
