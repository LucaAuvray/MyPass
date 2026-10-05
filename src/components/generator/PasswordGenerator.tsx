import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { usePasswordGenerator } from "@/hooks/usePasswordGenerator";
import { Button } from "@/components/ui/button";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Card, CardContent } from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Input } from "@/components/ui/input";
import { CopyButton } from "@/components/shared/CopyButton";
import { RefreshCw, Key, TextQuote } from "lucide-react";
import { cn } from "@/lib/utils";
import type { StrengthResult } from "@/lib/crypto";

interface PasswordGeneratorProps {
  onApply?: (password: string) => void;
  embedded?: boolean;
}

export function PasswordGenerator({ onApply, embedded }: PasswordGeneratorProps) {
  const { t } = useTranslation();
  const { generatePassword, generatePassphrase, evaluateStrength } = usePasswordGenerator();
  const [generated, setGenerated] = useState("");
  const [strength, setStrength] = useState<StrengthResult | null>(null);

  // Password tab
  const [length, setLength] = useState(20);
  const [useUpper, setUseUpper] = useState(true);
  const [useLower, setUseLower] = useState(true);
  const [useDigits, setUseDigits] = useState(true);
  const [useSymbols, setUseSymbols] = useState(true);

  // Passphrase tab
  const [wordCount, setWordCount] = useState(4);
  const [separator, setSeparator] = useState("-");
  const [wordCase, setWordCase] = useState<"lower" | "upper" | "title">("lower");

  const refreshPassword = useCallback(async () => {
    const pwd = await generatePassword({
      length,
      uppercase: useUpper,
      lowercase: useLower,
      digits: useDigits,
      symbols: useSymbols,
      excludeSimilar: false,
      excludeAmbiguous: false,
    });
    if (pwd) {
      setGenerated(pwd);
      const s = await evaluateStrength(pwd);
      if (s) setStrength(s);
    }
  }, [length, useUpper, useLower, useDigits, useSymbols, generatePassword, evaluateStrength]);

  const refreshPassphrase = useCallback(async () => {
    const phrase = await generatePassphrase({
      wordCount,
      separator,
      wordCase,
      includeNumber: false,
    });
    if (phrase) {
      setGenerated(phrase);
      const s = await evaluateStrength(phrase);
      if (s) setStrength(s);
    }
  }, [wordCount, separator, wordCase, generatePassphrase, evaluateStrength]);

  useEffect(() => {
    refreshPassword();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const strengthPercent = strength ? ((strength.score + 1) / 6) * 100 : 0;

  return (
    <Card className={cn(embedded && "border-none shadow-none")}>
      <CardContent className="space-y-4 p-4">
        {/* Generated output */}
        <div className="border-border bg-muted rounded-lg border p-3">
          <div className="flex items-center justify-between">
            <span className="font-mono text-sm">{generated || t("generator.clickRefresh")}</span>
            <div className="flex items-center gap-1">
              <CopyButton text={generated} />
              <Button variant="ghost" size="icon" className="size-7" onClick={refreshPassword}>
                <RefreshCw className="size-3.5" />
              </Button>
            </div>
          </div>
          {/* Strength bar */}
          {strength && (
            <div className="mt-2">
              <div className="bg-muted mb-1 h-1.5 overflow-hidden rounded-full">
                <div
                  className="h-full rounded-full transition-all duration-300"
                  style={{ width: `${strengthPercent}%`, backgroundColor: strength.color }}
                />
              </div>
              <p className="text-muted-foreground text-[10px]">
                {strength.label} — {strength.crackTimeDisplay}
              </p>
            </div>
          )}
        </div>

        <Tabs defaultValue="password">
          <TabsList className="w-full">
            <TabsTrigger value="password" className="flex-1 gap-1">
              <Key className="size-3.5" /> {t("generator.password")}
            </TabsTrigger>
            <TabsTrigger value="passphrase" className="flex-1 gap-1">
              <TextQuote className="size-3.5" /> {t("generator.passphrase")}
            </TabsTrigger>
          </TabsList>

          <TabsContent value="password" className="mt-3 space-y-3">
            <div className="space-y-1.5">
              <div className="flex items-center justify-between">
                <Label className="text-xs">
                  {t("generator.length")}: {length}
                </Label>
              </div>
              <Slider
                value={length}
                onValueChange={(v) => {
                  setLength(Array.isArray(v) ? v[0] : (v as number));
                  setTimeout(refreshPassword, 50);
                }}
                min={8}
                max={64}
                step={1}
              />
            </div>
            <div className="grid grid-cols-2 gap-2">
              <ToggleOption
                label="A-Z"
                checked={useUpper}
                onChange={(v) => {
                  setUseUpper(v);
                  setTimeout(refreshPassword, 50);
                }}
              />
              <ToggleOption
                label="a-z"
                checked={useLower}
                onChange={(v) => {
                  setUseLower(v);
                  setTimeout(refreshPassword, 50);
                }}
              />
              <ToggleOption
                label="0-9"
                checked={useDigits}
                onChange={(v) => {
                  setUseDigits(v);
                  setTimeout(refreshPassword, 50);
                }}
              />
              <ToggleOption
                label="!@#$"
                checked={useSymbols}
                onChange={(v) => {
                  setUseSymbols(v);
                  setTimeout(refreshPassword, 50);
                }}
              />
            </div>
          </TabsContent>

          <TabsContent value="passphrase" className="mt-3 space-y-3">
            <div className="space-y-1.5">
              <Label className="text-xs">
                {t("generator.words")}: {wordCount}
              </Label>
              <Slider
                value={wordCount}
                onValueChange={(v) => {
                  setWordCount(Array.isArray(v) ? v[0] : (v as number));
                  setTimeout(refreshPassphrase, 50);
                }}
                min={3}
                max={10}
                step={1}
              />
            </div>
            <div className="flex items-center gap-2">
              <Label className="text-xs">{t("generator.separator")}:</Label>
              <Input
                value={separator}
                onChange={(e) => {
                  setSeparator(e.target.value);
                  setTimeout(refreshPassphrase, 50);
                }}
                className="h-7 w-16 text-xs"
              />
              <Label className="ml-2 text-xs">{t("generator.case")}:</Label>
              <select
                value={wordCase}
                onChange={(e) => {
                  setWordCase(e.target.value as typeof wordCase);
                  setTimeout(refreshPassphrase, 50);
                }}
                className="border-input h-7 rounded-md border bg-transparent px-2 text-xs"
              >
                <option value="lower">{t("generator.lower")}</option>
                <option value="upper">{t("generator.upper")}</option>
                <option value="title">{t("generator.titleCase")}</option>
              </select>
            </div>
          </TabsContent>
        </Tabs>

        {onApply && (
          <Button className="w-full" size="sm" onClick={() => onApply(generated)}>
            {t("generator.apply")}
          </Button>
        )}
      </CardContent>
    </Card>
  );
}

function ToggleOption({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="border-border flex items-center justify-between rounded-lg border px-3 py-2">
      <Label className="text-xs">{label}</Label>
      <Switch checked={checked} onCheckedChange={onChange} />
    </div>
  );
}
