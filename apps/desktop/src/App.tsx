import { useEffect, useState } from "react";
import { appInfo, type AppInfo } from "./lib/ipc";

export default function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    appInfo()
      .then(setInfo)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  return (
    <main className="flex h-full flex-col items-center justify-center gap-3">
      <h1 className="text-3xl font-semibold tracking-tight">OpenTrainer</h1>
      <p className="text-sm text-zinc-400">
        Trainer + save editor for PC single-player games.
      </p>
      {info && (
        <p className="font-mono text-xs text-zinc-500" data-testid="version">
          v{info.version} · core {info.coreVersion}
        </p>
      )}
      {error && <p className="text-xs text-red-400">{error}</p>}
    </main>
  );
}
