import { useState } from "react";
import { splitGames } from "../lib/format";
import type { ProcessInfo, RunningGame } from "../lib/ipc";

interface Props {
  processes: ProcessInfo[];
  games: RunningGame[];
  /** The chosen PID as text ("" for none). */
  value: string;
  onChange: (pid: string) => void;
  onRefresh: () => void;
  label: string;
  className?: string;
}

/**
 * A dropdown of the player's running games, by name. Other programs are
 * listed only after "Show all running programs" is ticked (for a game
 * OpenTrainer didn't recognise).
 */
export function GamePicker({ processes, games, value, onChange, onRefresh, label, className }: Props) {
  const [showAll, setShowAll] = useState(false);
  const split = splitGames(processes, games);
  // Keep a chosen program listed even if it isn't a recognised game.
  const all = showAll || (value !== "" && !split.games.some((g) => String(g.process.pid) === value));
  const empty = split.games.length === 0;

  return (
    <div className="flex min-w-0 flex-1 flex-col gap-1">
      <select
        aria-label={label}
        className={className}
        value={value}
        onFocus={onRefresh}
        onChange={(e) => onChange(e.target.value)}
      >
        <option value="">{empty && !all ? "No game running: start one first" : "Pick your running game…"}</option>
        {split.games.map(({ game, process }) => (
          <option key={process.pid} value={process.pid}>
            {game.name} ({process.name}, {process.pid})
          </option>
        ))}
        {all && split.others.length > 0 && (
          <optgroup label="Other running programs">
            {split.others.map((p) => (
              <option key={p.pid} value={p.pid}>
                {p.name} ({p.pid})
              </option>
            ))}
          </optgroup>
        )}
      </select>
      {empty && !all && (
        <p className="text-xs text-zinc-500">
          Start the game (from the Games tab or as usual), then pick it here. Not listed? Tick the box below.
        </p>
      )}
      <label className="flex items-center gap-2 text-xs text-zinc-400">
        <input type="checkbox" checked={showAll} onChange={(e) => setShowAll(e.target.checked)} />
        Show all running programs
      </label>
    </div>
  );
}
