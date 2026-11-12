import { Component, type ErrorInfo, type ReactNode } from "react";
import { Mascot } from "./Icon";

interface State {
  error: Error | null;
}

/**
 * Keeps a screen's mistake inside that screen (v1.27.1). Without it, one error while drawing a
 * screen left the whole window blank until JuiceBox Trainer was closed and opened again. Now the
 * rest of the app keeps working, and "Try again" draws the screen afresh. Cheats that are on stay
 * on: they run in the background, not in the screen.
 */
export class ErrorBoundary extends Component<{ children: ReactNode; label?: string }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("A screen stopped:", error, info.componentStack);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div className="m-auto flex max-w-md flex-col items-center gap-3 p-6 text-center text-sm" role="alert">
        <Mascot mood="error" className="h-16 w-16" />
        <p className="text-base font-medium text-zinc-100">
          {this.props.label ?? "This screen"} ran into a problem
        </p>
        <p className="text-zinc-400">
          Your game and any cheats that are on keep running. Click Try again to show the screen again, or pick another tab.
        </p>
        <button className="btn-accent rounded-lg px-4 py-1.5 text-sm font-semibold" onClick={() => this.setState({ error: null })}>
          Try again
        </button>
        <p className="break-all font-mono text-xs text-zinc-600">{String(error.message ?? error).slice(0, 300)}</p>
      </div>
    );
  }
}
