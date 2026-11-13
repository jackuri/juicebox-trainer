import { useEffect, useState } from "react";
import { friendlyError } from "../lib/explain";
import { exeName } from "../lib/instant";
import { api, type EngineCheatsMade, type EngineSupport } from "../lib/ipc";

/**
 * Instant engine cheats (v1.26.0): for an Unreal game (and a Unity IL2CPP game, v1.37.0), make
 * cheats from the engine itself, with no pack, no AI and no value search. Shows nothing for games
 * on other engines.
 */
export function InstantCheats({
  processName,
  onMade,
}: {
  processName: string;
  onMade: (r: EngineCheatsMade) => void;
}) {
  const [support, setSupport] = useState<EngineSupport | null>(null);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    api.engineCheatsSupport().then(
      (s) => live && setSupport(s),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [processName]);

  if (!support?.engine) return null;
  if (!support.supported) {
    return support.why ? (
      <p className="text-xs text-zinc-500">
        Instant cheats: {support.why}
      </p>
    ) : null;
  }

  async function make() {
    setError(null);
    setWorking(true);
    try {
      // The game's name as the library knows it, if it's there.
      const games = await api.libraryList().catch(() => []);
      const mine = processName.toLowerCase();
      const name = (Array.isArray(games) ? games : []).find((g) => exeName(g.exe) === mine)?.name ?? "";
      onMade(await api.engineCheatsMake(name));
    } catch (e) {
      setError(friendlyError(e));
    } finally {
      setWorking(false);
    }
  }

  return (
    <section
      className="flex flex-col gap-2 rounded-xl bg-emerald-950/40 p-3 ring-1 ring-emerald-800/70"
      aria-label="Instant cheats"
    >
      <h3 className="text-sm font-semibold text-emerald-100">Instant cheats for {support.engine} games</h3>
      <p className="text-xs text-zinc-300">
        {support.engine?.startsWith("Unity")
          ? "JuiceBox can make cheats for this game right now from the game engine itself: game speed, and health, stamina, ammo or money when the game uses the usual names. Start playing first, so your character is in the world."
          : "JuiceBox can make cheats for this game right now from the game engine itself: game speed, move speed, jump height, gravity, and health, stamina, ammo or money when the game uses the usual names. Load your save first, so your character is in the world."}
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <button
          className="btn-accent rounded-lg px-3 py-1.5 text-xs font-semibold disabled:opacity-50"
          disabled={working}
          onClick={() => void make()}
        >
          {working ? "Reading the game…" : support.packKey ? "Make them again" : "Make instant cheats"}
        </button>
        {working && (
          <span className="text-xs text-zinc-400" role="status">
            This can take a few minutes the first time. You can keep playing; nothing in the game is changed yet.
          </span>
        )}
      </div>
      {error && (
        <p className="text-xs text-red-400" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
