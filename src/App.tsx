/**
 * The application root.
 *
 * The window renders immediately: the shell and the current screen draw from
 * whatever is already known, and each screen fills itself in. Nothing waits on
 * a filesystem scan before the interface appears.
 */
import * as React from "react";

import { StoreProvider, useStore } from "@/app/store";
import { AppShell } from "@/components/AppShell";
import { TooltipProvider } from "@/components/ui/overlay";
import { Logo } from "@/components/Logo";

import { OverviewView } from "@/views/Overview";
import { StorageMapView } from "@/views/StorageMap";
import { LargeFilesView } from "@/views/LargeFiles";
import { DuplicatesView } from "@/views/Duplicates";
import { CleanupView } from "@/views/Cleanup";
import { PerformanceView } from "@/views/Performance";
import { ProcessesView } from "@/views/Processes";
import { StartupView } from "@/views/Startup";
import { ApplicationsView } from "@/views/Applications";
import { BatteryView } from "@/views/Battery";
import { DriveHealthView } from "@/views/DriveHealth";
import { AssistantView } from "@/views/Assistant";
import { ActivityView } from "@/views/Activity";
import { SettingsView } from "@/views/Settings";
import { FirstRunView } from "@/views/FirstRun";

/**
 * A screen that throws must not take the whole window with it. The boundary
 * keeps the shell alive so the user can navigate somewhere else.
 */
class ScreenBoundary extends React.Component<
  { children: React.ReactNode; screen: string },
  { error: Error | null }
> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidUpdate(previous: { screen: string }) {
    if (previous.screen !== this.props.screen && this.state.error) {
      this.setState({ error: null });
    }
  }

  render() {
    if (this.state.error) {
      return (
        <div className="panel p-6">
          <h2 className="text-sm font-semibold text-[var(--color-ink)]">
            This screen could not be displayed
          </h2>
          <p className="mt-1.5 text-xs text-[var(--color-ink-muted)]">
            {this.state.error.message}
          </p>
          <p className="mt-3 text-2xs text-[var(--color-ink-subtle)]">
            The rest of AllInsight is unaffected. Choose another screen from the sidebar, or reopen
            this one.
          </p>
        </div>
      );
    }
    return this.props.children;
  }
}

function Screen() {
  const { route } = useStore();
  switch (route) {
    case "overview":
      return <OverviewView />;
    case "storage-map":
      return <StorageMapView />;
    case "large-files":
      return <LargeFilesView />;
    case "duplicates":
      return <DuplicatesView />;
    case "cleanup":
      return <CleanupView />;
    case "performance":
      return <PerformanceView />;
    case "processes":
      return <ProcessesView />;
    case "startup":
      return <StartupView />;
    case "applications":
      return <ApplicationsView />;
    case "battery":
      return <BatteryView />;
    case "drive-health":
      return <DriveHealthView />;
    case "assistant":
      return <AssistantView />;
    case "activity":
      return <ActivityView />;
    case "settings":
      return <SettingsView />;
    default:
      return <OverviewView />;
  }
}

function Splash() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 bg-[var(--color-canvas)]">
      <Logo size={40} />
      <p className="font-display text-xs tracking-[0.3em] text-[var(--color-ink-subtle)]">
        AllInsight
      </p>
    </div>
  );
}

function Root() {
  const { ready, settings, route } = useStore();
  const [firstRunDone, setFirstRunDone] = React.useState(false);

  if (!ready) return <Splash />;

  if (settings && !settings.first_run_complete && !firstRunDone) {
    return <FirstRunView onDone={() => setFirstRunDone(true)} />;
  }

  return (
    <AppShell>
      <ScreenBoundary screen={route}>
        <Screen />
      </ScreenBoundary>
    </AppShell>
  );
}

export default function App() {
  return (
    <StoreProvider>
      <TooltipProvider>
        <Root />
      </TooltipProvider>
    </StoreProvider>
  );
}
