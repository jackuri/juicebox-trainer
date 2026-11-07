import { useState } from "react";
import { hex } from "../lib/format";
import { api, type CommandError, isSessionLost, type Signature, toCommandError } from "../lib/ipc";

interface Props {
  onSessionLost: (e: CommandError) => void;
}

/**
 * Signature (AOB) maker: turns a code address into a byte pattern that finds
 * the same code again after the game restarts or updates.
 */
export function SignaturePanel({ onSessionLost }: Props) {
  const [address, setAddress] = useState("");
  const [sig, setSig] = useState<Signature | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  async function make() {
    setError(null);
    setCopied(false);
    try {
      setSig(await api.signatureMake(address.trim()));
    } catch (e) {
      const err = toCommandError(e);
      if (isSessionLost(err)) onSessionLost(err);
      else {
        setSig(null);
        setError(err.message);
      }
    }
  }

  async function copy() {
    if (!sig) return;
    try {
      await navigator.clipboard.writeText(sig.pattern);
      setCopied(true);
    } catch {
      setError("Could not copy; select the pattern and copy it by hand.");
    }
  }

  return (
    <section className="flex flex-col gap-3" aria-label="Signature maker">
      <p className="text-xs text-zinc-400">
        Enter the address of an instruction in the game's code (e.g. <code>testbed_native.exe+0x1234</code>). The
        signature finds that code again later; jump targets and addresses are wildcarded (<code>??</code>).
      </p>
      <div className="flex flex-wrap items-end gap-2">
        <label className="flex flex-col gap-1 text-xs text-zinc-400">
          Code address
          <input
            className="w-72 rounded-lg bg-zinc-900 px-3 py-1.5 font-mono text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
            placeholder="game.exe+0x1234"
            aria-label="Code address"
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && address.trim() && void make()}
          />
        </label>
        <button
          className="rounded-lg btn-accent px-4 py-1.5 text-sm font-semibold disabled:opacity-40"
          disabled={!address.trim()}
          onClick={() => void make()}
        >
          Make signature
        </button>
      </div>

      {error && (
        <p className="text-xs text-red-400" role="alert">
          {error}
        </p>
      )}

      {sig && (
        <div className="flex flex-col gap-2 rounded-lg bg-zinc-900/60 p-3">
          <div className="flex items-start gap-2">
            <code className="flex-1 break-all font-mono text-sm text-emerald-300" data-testid="signature">
              {sig.pattern}
            </code>
            <button className="rounded bg-zinc-800 px-2 py-0.5 text-xs hover:bg-zinc-700" onClick={() => void copy()}>
              {copied ? "Copied" : "Copy"}
            </button>
          </div>
          <p className="text-xs text-zinc-400">
            {sig.len} bytes, unique in {sig.module} (starts at {sig.module}+{hex(sig.moduleOffset)})
          </p>
          <ol className="font-mono text-xs text-zinc-300" aria-label="Instructions">
            {sig.instructions.map((i) => (
              <li key={i.offset}>
                <span className="text-zinc-500">+{i.offset.toString(16).toUpperCase().padStart(2, "0")}</span>{" "}
                {i.text}
              </li>
            ))}
          </ol>
        </div>
      )}
    </section>
  );
}
