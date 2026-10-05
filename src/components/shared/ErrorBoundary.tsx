import { Component, type ReactNode } from "react";
import { AlertTriangle, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import i18n from "@/i18n/config";

interface ErrorBoundaryProps {
  children: ReactNode;
  fallback?: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error("MyPass Error:", error, errorInfo);
  }

  handleRetry = () => {
    this.setState({ hasError: false, error: null });
  };

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) return this.props.fallback;

      return (
        <div className="bg-background flex min-h-screen flex-col items-center justify-center p-4">
          <div className="flex flex-col items-center gap-4 text-center">
            <div className="bg-destructive/10 flex size-16 items-center justify-center rounded-2xl">
              <AlertTriangle className="text-destructive size-8" />
            </div>
            <h2 className="text-xl font-bold">{i18n.t("common.somethingWrong")}</h2>
            <p className="text-muted-foreground max-w-sm text-sm">
              {this.state.error?.message || i18n.t("common.unexpectedError")}
            </p>
            <Button onClick={this.handleRetry} className="gap-1.5">
              <RefreshCw className="size-4" />
              {i18n.t("common.tryAgain")}
            </Button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
