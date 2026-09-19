/**
 * Application-wide state.
 *
 * Deliberately small: settings, the current screen, scan progress, and the
 * toast queue. Screen data is fetched by the screen that needs it, so opening
 * one view never pays for the others.
 */
import * as React from "react";
import { api, AllInsightError, onAlert, onScanComplete, onScanProgress } from "@/lib/api";
import type { Alert, EnvironmentInfo, ScanProgressSnapshot, Settings } from "@/lib/types";
import { ROUTES, type RouteId } from "./navigation";

// Guards the stored route against a value that is no longer a real screen.
const ROUTE_IDS = new Set<RouteId>(ROUTES.map((r) => r.id));

export interface Toast {
  id: number;
  tone: "info" | "success" | "warning" | "error";
  title: string;
  body?: string;
}

interface StoreValue {
  ready: boolean;
  settings: Settings | null;
  environment: EnvironmentInfo | null;
  route: RouteId;
  navigate: (route: RouteId) => void;

  scanProgress: ScanProgressSnapshot | null;
  scanning: boolean;
  /** Bumped whenever a background scan finishes, so screens can refetch. */
  scanGeneration: number;

  toasts: Toast[];
  toast: (toast: Omit<Toast, "id">) => void;
  dismissToast: (id: number) => void;
  reportError: (error: unknown, fallback?: string) => void;

  saveSettings: (next: Settings) => Promise<void>;
  refreshSettings: () => Promise<void>;
}

const StoreContext = React.createContext<StoreValue | null>(null);

export function useStore(): StoreValue {
  const value = React.useContext(StoreContext);
  if (!value) throw new Error("useStore must be used inside StoreProvider");
  return value;
}

let toastCounter = 0;

export function StoreProvider({ children }: { children: React.ReactNode }) {
  const [ready, setReady] = React.useState(false);
  const [settings, setSettings] = React.useState<Settings | null>(null);
  const [environment, setEnvironment] = React.useState<EnvironmentInfo | null>(null);
  const [route, setRoute] = React.useState<RouteId>("overview");

  // Reopening on the screen you left is worth the one small write it costs,
  // and it is stored under its own key so navigating never rewrites the whole
  // settings document.
  const navigate = React.useCallback((next: RouteId) => {
    setRoute(next);
    api.setLastRoute(next).catch(() => {
      // Failing to remember the screen is not worth telling anyone about.
    });
  }, []);
  const [scanProgress, setScanProgress] = React.useState<ScanProgressSnapshot | null>(null);
  const [scanning, setScanning] = React.useState(false);
  const [scanGeneration, setScanGeneration] = React.useState(0);
  const [toasts, setToasts] = React.useState<Toast[]>([]);
  const lastProgressAt = React.useRef(0);

  const dismissToast = React.useCallback((id: number) => {
    setToasts((current) => current.filter((t) => t.id !== id));
  }, []);

  const toast = React.useCallback(
    (next: Omit<Toast, "id">) => {
      const id = ++toastCounter;
      setToasts((current) => [...current.slice(-3), { ...next, id }]);
      // Errors stay until dismissed; everything else clears itself.
      if (next.tone !== "error") {
        setTimeout(() => dismissToast(id), 5200);
      }
    },
    [dismissToast],
  );

  const reportError = React.useCallback(
    (error: unknown, fallback = "That did not work.") => {
      const message =
        error instanceof AllInsightError || error instanceof Error ? error.message : fallback;
      toast({ tone: "error", title: message });
    },
    [toast],
  );

  const refreshSettings = React.useCallback(async () => {
    const next = await api.getSettings();
    setSettings(next);
  }, []);

  const saveSettings = React.useCallback(
    async (next: Settings) => {
      try {
        const saved = await api.saveSettings(next);
        setSettings(saved);
      } catch (error) {
        reportError(error, "Those settings could not be saved.");
        throw error;
      }
    },
    [reportError],
  );

  // Initial load. The window is already visible at this point; this only fills
  // it in, so a slow first call never blocks the first frame.
  React.useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [loadedSettings, loadedEnvironment, lastRoute] = await Promise.all([
          api.getSettings(),
          api.getEnvironment(),
          api.getLastRoute().catch(() => null),
        ]);
        if (cancelled) return;
        setSettings(loadedSettings);
        setEnvironment(loadedEnvironment);
        if (lastRoute && ROUTE_IDS.has(lastRoute as RouteId)) {
          setRoute(lastRoute as RouteId);
        }

        // Scanning on launch is off by default and opt-in, so honouring it
        // here is doing what was asked rather than being presumptuous. It
        // starts after the interface is up, and only when the system volume
        // has not already been scanned this session.
        if (loadedSettings.scan_on_launch) {
          const overview = await api.getStorageOverview();
          const root = overview.system_volume ?? overview.volumes[0]?.mount_point;
          const already = root ? await api.getScanSummary(root) : null;
          if (root && !already && !cancelled) {
            await api.scanDirectory(root).catch(() => {
              // A scan that will not start is not a reason to fail startup.
            });
          }
        }
      } catch (error) {
        if (!cancelled) reportError(error, "AllInsight could not read its settings.");
      } finally {
        if (!cancelled) setReady(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [reportError]);

  // Backend events.
  React.useEffect(() => {
    const unlisteners: Promise<() => void>[] = [
      onScanProgress((progress) => {
        setScanProgress(progress);
        setScanning(!progress.finished && !progress.cancelled);
        lastProgressAt.current = Date.now();
      }),
      onScanComplete(() => {
        setScanning(false);
        setScanProgress(null);
        setScanGeneration((n) => n + 1);
      }),
      onAlert((alert: Alert) => {
        toast({
          tone:
            alert.severity === "critical"
              ? "error"
              : alert.severity === "warning"
                ? "warning"
                : "info",
          title: alert.title,
          body: alert.body,
        });
      }),
    ];
    return () => {
      unlisteners.forEach((p) => p.then((un) => un()).catch(() => {}));
    };
  }, [toast]);

  // A watchdog for the scan indicator.
  //
  // The progress strip is driven by events, and an event that never arrives
  // would leave it spinning forever. If nothing has been heard for a few
  // seconds, ask the backend directly whether a scan is actually running and
  // clear the indicator when it is not.
  React.useEffect(() => {
    if (!scanning) return;
    const timer = window.setInterval(async () => {
      if (Date.now() - lastProgressAt.current < 6000) return;
      try {
        const live = await api.getScanProgress();
        if (!live || live.cancelled || live.finished) {
          setScanning(false);
          setScanProgress(null);
          setScanGeneration((n) => n + 1);
        } else {
          lastProgressAt.current = Date.now();
        }
      } catch {
        setScanning(false);
        setScanProgress(null);
      }
    }, 3000);
    return () => window.clearInterval(timer);
  }, [scanning]);

  // Theme, scale and motion are applied to the document root so every
  // component picks them up from the tokens rather than from props.
  React.useEffect(() => {
    if (!settings) return;
    const root = document.documentElement;
    // Themes that paint on a dark ground, so the browser knows which set of
    // built-in colours (scrollbars, form controls) to pair with them.
    const DARK_THEMES = new Set(["dark", "midnight", "contrast"]);

    const applyTheme = () => {
      const resolved =
        settings.theme === "system"
          ? window.matchMedia("(prefers-color-scheme: light)").matches
            ? "light"
            : "dark"
          : settings.theme;
      root.setAttribute("data-theme", resolved);
      root.style.colorScheme = DARK_THEMES.has(resolved) ? "dark" : "light";
    };
    applyTheme();

    root.style.fontSize = `${(settings.ui_scale / 100) * 16}px`;
    root.setAttribute("data-motion", settings.reduce_motion ? "reduced" : "full");

    if (settings.theme === "system") {
      const media = window.matchMedia("(prefers-color-scheme: light)");
      media.addEventListener("change", applyTheme);
      return () => media.removeEventListener("change", applyTheme);
    }
  }, [settings]);

  const value = React.useMemo<StoreValue>(
    () => ({
      ready,
      settings,
      environment,
      route,
      navigate,
      scanProgress,
      scanning,
      scanGeneration,
      toasts,
      toast,
      dismissToast,
      reportError,
      saveSettings,
      refreshSettings,
    }),
    [
      ready,
      settings,
      environment,
      route,
      navigate,
      scanProgress,
      scanning,
      scanGeneration,
      toasts,
      toast,
      dismissToast,
      reportError,
      saveSettings,
      refreshSettings,
    ],
  );

  return <StoreContext.Provider value={value}>{children}</StoreContext.Provider>;
}

/**
 * Fetch on mount, with loading and error state.
 *
 * Screens use this instead of hand-rolling effects, so every screen fails the
 * same way: a message the user can read, and a retry.
 */
export function useAsync<T>(
  loader: () => Promise<T>,
  deps: React.DependencyList = [],
): {
  data: T | null;
  loading: boolean;
  error: string | null;
  reload: () => void;
} {
  const [data, setData] = React.useState<T | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);
  const [nonce, setNonce] = React.useState(0);

  React.useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    loader()
      .then((result) => {
        if (!cancelled) setData(result);
      })
      .catch((e) => {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : "That could not be loaded.");
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, nonce]);

  return { data, loading, error, reload: () => setNonce((n) => n + 1) };
}

/** Poll a value on an interval. Used for live metrics, never for scans. */
export function usePolled<T>(
  loader: () => Promise<T>,
  intervalMs: number,
  enabled = true,
): T | null {
  const [data, setData] = React.useState<T | null>(null);
  const saved = React.useRef(loader);
  saved.current = loader;

  React.useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    let timer: number | undefined;

    const tick = async () => {
      // The window hides to the notification area rather than closing, and a
      // hidden window has nothing to draw. Skip the call and look again after
      // the usual interval; the visibility listener below wakes it sooner.
      if (document.visibilityState === "hidden") {
        if (!cancelled) timer = window.setTimeout(tick, intervalMs);
        return;
      }
      try {
        const result = await saved.current();
        if (!cancelled) setData(result);
      } catch {
        // A single failed sample is not worth interrupting anyone for; the
        // next tick will either succeed or the screen will show stale data
        // with its own timestamp.
      }
      if (!cancelled) timer = window.setTimeout(tick, intervalMs);
    };

    // Coming back into view samples at once, so the first thing on screen is
    // current rather than however old the last sample happened to be.
    const onVisible = () => {
      if (document.visibilityState !== "visible" || cancelled) return;
      if (timer) window.clearTimeout(timer);
      tick();
    };
    document.addEventListener("visibilitychange", onVisible);

    tick();
    return () => {
      cancelled = true;
      document.removeEventListener("visibilitychange", onVisible);
      if (timer) window.clearTimeout(timer);
    };
  }, [intervalMs, enabled]);

  return data;
}
