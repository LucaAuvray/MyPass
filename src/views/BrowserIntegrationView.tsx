import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { tauriCommand } from "@/lib/tauri";
import { Globe, Chrome, CheckCircle, XCircle, Shield, ExternalLink } from "lucide-react";

interface BrowserInfo {
  name: string;
  connected: boolean;
  associated: boolean;
  last_seen: string;
}

interface BrowserStatus {
  connected: boolean;
  browsers: BrowserInfo[];
}

export function BrowserIntegrationView() {
  const { t } = useTranslation();
  const [enabled, setEnabled] = useState(false);
  const [status, setStatus] = useState<BrowserStatus | null>(null);
  const browsers = [
    {
      name: "Google Chrome",
      icon: Chrome,
      connected: false,
      associated: false,
      storeUrl: "https://chrome.google.com/webstore",
    },
    {
      name: "Mozilla Firefox",
      icon: Globe,
      connected: false,
      associated: false,
      storeUrl: "https://addons.mozilla.org",
    },
    {
      name: "Microsoft Edge",
      icon: Globe,
      connected: false,
      associated: false,
      storeUrl: "https://microsoftedge.microsoft.com/addons",
    },
  ];

  useEffect(() => {
    tauriCommand<boolean>("is_browser_integration_enabled")
      .then(setEnabled)
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (!enabled) return;
    tauriCommand<BrowserStatus>("get_browser_status")
      .then(setStatus)
      .catch(() => {});
  }, [enabled]);

  const handleToggle = async (checked: boolean) => {
    setEnabled(checked);
    try {
      await tauriCommand("toggle_browser_integration", { enabled: checked });
    } catch {
      setEnabled(!checked);
    }
  };

  return (
    <div className="space-y-6 p-4 md:p-6">
      <div>
        <h1 className="font-display text-2xl font-bold tracking-tight">{t("browser.title")}</h1>
        <p className="text-muted-foreground mt-1 text-sm">{t("browser.subtitle")}</p>
      </div>
      <Card>
        <CardContent className="flex items-center justify-between p-6">
          <div className="space-y-1">
            <div className="flex items-center gap-2">
              <Shield className="text-primary size-4" />
              <h3 className="text-sm font-semibold">{t("browser.enable")}</h3>
            </div>
            <p className="text-muted-foreground text-xs">{t("browser.enableDesc")}</p>
          </div>
          <Switch checked={enabled} onCheckedChange={handleToggle} />
        </CardContent>
      </Card>
      {enabled && (
        <div className="space-y-4">
          <h2 className="text-lg font-semibold">{t("browser.associatedBrowsers")}</h2>
          {status && status.browsers.length > 0 ? (
            status.browsers.map((browser) => (
              <Card key={browser.name}>
                <CardContent className="flex items-center gap-4 p-4">
                  <div className="bg-muted flex size-10 shrink-0 items-center justify-center rounded-xl">
                    <Globe className="size-5" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <h4 className="truncate text-sm font-semibold">{browser.name}</h4>
                      {browser.connected ? (
                        <Badge variant="secondary" className="gap-1 text-[10px] text-green-600">
                          <CheckCircle className="size-2.5" />
                          {t("browser.connected")}
                        </Badge>
                      ) : (
                        <Badge
                          variant="secondary"
                          className="text-muted-foreground gap-1 text-[10px]"
                        >
                          <XCircle className="size-2.5" />
                          {t("browser.disconnected")}
                        </Badge>
                      )}
                    </div>
                    {browser.last_seen && (
                      <p className="text-muted-foreground text-xs">
                        {t("browser.associatedOn", {
                          date: new Date(Number(browser.last_seen) * 1000).toLocaleString(),
                        })}
                      </p>
                    )}
                  </div>
                </CardContent>
              </Card>
            ))
          ) : (
            <p className="text-muted-foreground text-sm">{t("browser.noBrowsers")}</p>
          )}
        </div>
      )}
      {enabled && (
        <div className="space-y-4">
          <h2 className="text-lg font-semibold">{t("browser.connectedBrowsers")}</h2>
          {browsers.map((browser) => (
            <Card key={browser.name}>
              <CardContent className="flex items-center gap-4 p-4">
                <div className="bg-muted flex size-10 shrink-0 items-center justify-center rounded-xl">
                  <browser.icon className="size-5" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <h4 className="text-sm font-semibold">{browser.name}</h4>
                    {browser.connected ? (
                      <Badge variant="secondary" className="gap-1 text-[10px] text-green-600">
                        <CheckCircle className="size-2.5" />
                        {t("browser.connected")}
                      </Badge>
                    ) : (
                      <Badge
                        variant="secondary"
                        className="text-muted-foreground gap-1 text-[10px]"
                      >
                        <XCircle className="size-2.5" />
                        {t("browser.disconnected")}
                      </Badge>
                    )}
                  </div>
                  <p className="text-muted-foreground text-xs">{t("browser.installDesc")}</p>
                </div>
                <a
                  href={browser.storeUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="border-input hover:bg-muted inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-sm font-medium transition-colors"
                >
                  <ExternalLink className="size-3.5" />
                  {t("browser.install")}
                </a>
              </CardContent>
            </Card>
          ))}
        </div>
      )}
      <Separator />
      <Card>
        <CardHeader>
          <CardTitle className="text-base">{t("browser.howItWorks")}</CardTitle>
          <CardDescription>{t("browser.protocolDesc")}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <Step number={1} title={t("browser.step1Title")} description={t("browser.step1Desc")} />
          <Step number={2} title={t("browser.step2Title")} description={t("browser.step2Desc")} />
          <Step number={3} title={t("browser.step3Title")} description={t("browser.step3Desc")} />
          <Step number={4} title={t("browser.step4Title")} description={t("browser.step4Desc")} />
        </CardContent>
      </Card>
      <div className="border-border bg-muted/30 flex items-start gap-3 rounded-xl border p-4">
        <Shield className="text-primary size-5 shrink-0" />
        <div>
          <h4 className="text-sm font-semibold">{t("browser.encrypted")}</h4>
          <p className="text-muted-foreground text-xs">{t("browser.encryptedDesc")}</p>
        </div>
      </div>
      <p className="text-muted-foreground text-center text-xs">
        {t("browser.version", { version: __APP_VERSION__, date: __BUILD_DATE__ })}
      </p>
    </div>
  );
}
function Step({
  number,
  title,
  description,
}: {
  number: number;
  title: string;
  description: string;
}) {
  return (
    <div className="flex gap-3">
      <div className="bg-primary text-primary-foreground flex size-7 shrink-0 items-center justify-center rounded-full text-xs font-bold">
        {number}
      </div>
      <div>
        <h5 className="text-sm font-semibold">{title}</h5>
        <p className="text-muted-foreground text-xs">{description}</p>
      </div>
    </div>
  );
}
