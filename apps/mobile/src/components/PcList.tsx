import type { Pc } from "../lib/ipc";

/** The PCs this phone is paired with. */
export function PcList({ pcs, onOpen, onPair }: { pcs: Pc[]; onOpen: (pc: Pc) => void; onPair: () => void }) {
  return (
    <main className="flex flex-col gap-4 p-4">
      <header className="flex items-center gap-3 pt-2">
        <img src="/default.png" alt="" className="h-12 w-12" />
        <div>
          <h1 className="text-xl font-semibold">JuiceBox Remote</h1>
          <p className="text-sm text-zinc-400">Cheats on your PC, from your phone</p>
        </div>
      </header>
      <h2 className="text-xs font-semibold uppercase tracking-wider text-zinc-500">Your PCs</h2>
      <ul className="flex flex-col gap-2">
        {pcs.map((pc) => (
          <li key={pc.id}>
            <button
              className="flex w-full items-center gap-3 rounded-2xl bg-zinc-900 px-4 py-4 text-left ring-1 ring-zinc-800 active:bg-zinc-800"
              onClick={() => onOpen(pc)}
            >
              <span aria-hidden className="text-2xl">
                🖥️
              </span>
              <span className="flex min-w-0 flex-col">
                <span className="truncate text-base font-medium">{pc.name}</span>
                <span className="text-xs text-zinc-500">{pc.address}</span>
              </span>
              <span aria-hidden className="ml-auto text-xl text-zinc-500">
                ›
              </span>
            </button>
          </li>
        ))}
      </ul>
      <button className="btn-accent rounded-2xl px-4 py-3 text-base font-medium" onClick={onPair}>
        Pair another PC
      </button>
    </main>
  );
}
