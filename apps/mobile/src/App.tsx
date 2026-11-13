import { useCallback, useEffect, useState } from "react";
import { Pair } from "./components/Pair";
import { PcList } from "./components/PcList";
import { Remote } from "./components/Remote";
import { api, explain, prefs, type Pc } from "./lib/ipc";

type Screen = { name: "loading" } | { name: "pcs" } | { name: "pair" } | { name: "remote"; pc: Pc };

/** JuiceBox Remote: the paired PCs, pairing a new one, and the remote for one PC. */
export default function App() {
  const [screen, setScreen] = useState<Screen>({ name: "loading" });
  const [pcs, setPcs] = useState<Pc[]>([]);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const list = await api.pcs();
      setPcs(list);
      setError(null);
      return list;
    } catch (e) {
      setError(explain(e));
      return [];
    }
  }, []);

  /** Shows a screen; Android's back button comes back to the PC list (popstate below). */
  const go = useCallback((next: Screen) => {
    setScreen(next);
    prefs.set("lastPc", next.name === "remote" ? next.pc.id : null);
    if (next.name === "remote" || next.name === "pair") window.history.pushState({ screen: next.name }, "");
  }, []);

  const showList = useCallback(() => {
    setScreen({ name: "pcs" });
    prefs.set("lastPc", null);
    void reload();
  }, [reload]);

  useEffect(() => {
    const t = setTimeout(
      () =>
        void reload().then((list) => {
          const last = list.find((p) => p.id === prefs.get("lastPc"));
          if (last) go({ name: "remote", pc: last });
          else if (list.length === 0) setScreen({ name: "pair" });
          else setScreen({ name: "pcs" });
        }),
      0,
    );
    return () => clearTimeout(t);
  }, [reload, go]);

  useEffect(() => {
    window.addEventListener("popstate", showList);
    return () => window.removeEventListener("popstate", showList);
  }, [showList]);

  const back = () => {
    if (window.history.state) window.history.back();
    else showList();
  };

  return (
    <div className="mx-auto flex min-h-full max-w-xl flex-col">
      {error && (
        <p role="alert" className="m-4 rounded-xl bg-red-950/60 px-3 py-2 text-sm text-red-200">
          {error}
        </p>
      )}
      {screen.name === "pcs" && (
        <PcList pcs={pcs} onOpen={(pc) => go({ name: "remote", pc })} onPair={() => go({ name: "pair" })} />
      )}
      {screen.name === "pair" && (
        <Pair
          canGoBack={pcs.length > 0}
          onBack={back}
          onPaired={(pc) => {
            void reload();
            setScreen({ name: "remote", pc });
            prefs.set("lastPc", pc.id);
          }}
        />
      )}
      {screen.name === "remote" && (
        <Remote
          key={screen.pc.id}
          pc={screen.pc}
          onBack={back}
          onForgotten={() => {
            prefs.set("lastPc", null);
            void reload().then((list) => setScreen(list.length ? { name: "pcs" } : { name: "pair" }));
          }}
        />
      )}
    </div>
  );
}
