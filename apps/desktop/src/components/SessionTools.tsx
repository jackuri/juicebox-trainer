import { useState } from "react";
import { hex } from "../lib/format";
import {
  api,
  type CommandError,
  isSessionLost,
  type ListEntry,
  type SessionInfo,
  toCommandError,
  type ValueType,
} from "../lib/ipc";
import { AddressList } from "./AddressList";
import { FindValuePanel } from "./FindValuePanel";
import { MemoryPanel } from "./MemoryPanel";
import { ModuleTable } from "./ModuleTable";
import { PointerPanel, type PointerRequest } from "./PointerPanel";
import { ScannerPanel } from "./ScannerPanel";
import { SignaturePanel } from "./SignaturePanel";
import { TrainerPanel } from "./TrainerPanel";

const TABS = ["Trainer", "Find a value", "Scanner", "Pointers", "Signatures", "Read / write", "Modules"] as const;
type Tab = (typeof TABS)[number];

interface Props {
  session: SessionInfo;
  /** The game's name (from the library), for packs made here. */
  gameName?: string;
  onSessionLost: (e: CommandError) => void;
  onRefreshModules: () => void;
  /** Opens the Packs tab. */
  onShowPacks?: () => void;
}

/**
 * Tools shown while attached: trainer (packs), Find a value (guided), scanner, pointer scanner,
 * signature maker, read/write, modules, address list.
 */
export function SessionTools({ session, gameName, onSessionLost, onRefreshModules, onShowPacks }: Props) {
  const [tab, setTab] = useState<Tab>("Trainer");
  const [entries, setEntries] = useState<ListEntry[]>([]);
  const [pointerRequest, setPointerRequest] = useState<PointerRequest | null>(null);

  async function addToList(address: number | string, ty: ValueType) {
    try {
      await api.addressAdd(typeof address === "number" ? hex(address) : address, ty, "");
      setEntries(await api.addressList());
    } catch (e) {
      const err = toCommandError(e);
      if (isSessionLost(err)) onSessionLost(err);
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div role="tablist" aria-label="Tools" className="flex flex-wrap gap-1 self-start rounded-xl bg-zinc-900/80 p-1 ring-1 ring-zinc-800">
        {TABS.map((t) => (
          <button
            key={t}
            role="tab"
            aria-selected={tab === t}
            onClick={() => setTab(t)}
            className={`rounded-lg px-3.5 py-1.5 text-sm font-medium transition-colors ${
              tab === t ? "bg-zinc-800 text-white shadow ring-1 ring-zinc-700" : "text-zinc-400 hover:text-zinc-100"
            }`}
          >
            {t}
          </button>
        ))}
      </div>

      {/* Panels stay mounted so a scan or typed value survives switching tabs. */}
      <div hidden={tab !== "Trainer"}>
        <TrainerPanel session={session} onSessionLost={onSessionLost} onShowPacks={onShowPacks} />
      </div>
      <div hidden={tab !== "Find a value"}>
        <FindValuePanel
          session={session}
          gameName={gameName ?? session.process.name.replace(/\.exe$/i, "")}
          onSessionLost={onSessionLost}
          onListChanged={() => void api.addressList().then(setEntries, () => undefined)}
          onShowTrainer={() => setTab("Trainer")}
        />
      </div>
      <div hidden={tab !== "Scanner"}>
        <ScannerPanel
          active={tab === "Scanner"}
          onAddToList={(a, t) => void addToList(a, t)}
          onFindPointers={(address, ty) => {
            setPointerRequest({ address, ty });
            setTab("Pointers");
          }}
          onSessionLost={onSessionLost}
        />
      </div>
      <div hidden={tab !== "Pointers"}>
        <PointerPanel
          onAddToList={(e, t) => void addToList(e, t)}
          onSessionLost={onSessionLost}
          request={pointerRequest}
        />
      </div>
      <div hidden={tab !== "Signatures"}>
        <SignaturePanel onSessionLost={onSessionLost} />
      </div>
      <div hidden={tab !== "Read / write"}>
        <MemoryPanel onSessionLost={onSessionLost} />
      </div>
      <div hidden={tab !== "Modules"} className="h-80">
        <ModuleTable modules={session.modules} onRefresh={onRefreshModules} />
      </div>

      <AddressList entries={entries} onChange={setEntries} onSessionLost={onSessionLost} />
    </div>
  );
}
