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
import { MemoryPanel } from "./MemoryPanel";
import { ModuleTable } from "./ModuleTable";
import { PointerPanel } from "./PointerPanel";
import { ScannerPanel } from "./ScannerPanel";
import { SignaturePanel } from "./SignaturePanel";
import { TrainerPanel } from "./TrainerPanel";

const TABS = ["Trainer", "Scanner", "Pointers", "Signatures", "Read / write", "Modules"] as const;
type Tab = (typeof TABS)[number];

interface Props {
  session: SessionInfo;
  onSessionLost: (e: CommandError) => void;
  onRefreshModules: () => void;
}

/**
 * Tools shown while attached: trainer (packs), scanner, pointer scanner,
 * signature maker, read/write, modules, address list.
 */
export function SessionTools({ session, onSessionLost, onRefreshModules }: Props) {
  const [tab, setTab] = useState<Tab>("Trainer");
  const [entries, setEntries] = useState<ListEntry[]>([]);

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
      <div role="tablist" aria-label="Tools" className="flex gap-1 border-b border-zinc-800">
        {TABS.map((t) => (
          <button
            key={t}
            role="tab"
            aria-selected={tab === t}
            onClick={() => setTab(t)}
            className={`-mb-px border-b-2 px-3 py-1.5 text-sm ${
              tab === t
                ? "border-emerald-500 text-zinc-100"
                : "border-transparent text-zinc-400 hover:text-zinc-200"
            }`}
          >
            {t}
          </button>
        ))}
      </div>

      {/* Panels stay mounted so a scan or typed value survives switching tabs. */}
      <div hidden={tab !== "Trainer"}>
        <TrainerPanel session={session} onSessionLost={onSessionLost} />
      </div>
      <div hidden={tab !== "Scanner"}>
        <ScannerPanel onAddToList={(a, t) => void addToList(a, t)} onSessionLost={onSessionLost} />
      </div>
      <div hidden={tab !== "Pointers"}>
        <PointerPanel onAddToList={(e, t) => void addToList(e, t)} onSessionLost={onSessionLost} />
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
