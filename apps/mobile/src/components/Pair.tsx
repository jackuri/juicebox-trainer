import { useState } from "react";
import { api, explain, prefs, scanQr, type Pc } from "../lib/ipc";

/** Pairing with a PC: scan its QR code (or paste its link). */
export function Pair({
  canGoBack,
  onBack,
  onPaired,
}: {
  canGoBack: boolean;
  onBack: () => void;
  onPaired: (pc: Pc) => void;
}) {
  const [phone, setPhone] = useState(() => prefs.get("phoneName") ?? "My phone");
  const [link, setLink] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pasting, setPasting] = useState(false);

  async function pair(text: string) {
    setBusy(true);
    setError(null);
    try {
      const name = phone.trim() || "My phone";
      prefs.set("phoneName", name);
      onPaired(await api.pair(text, name));
    } catch (e) {
      setError(explain(e));
    } finally {
      setBusy(false);
    }
  }

  async function scan() {
    setError(null);
    const text = await scanQr();
    if (text) await pair(text);
    else {
      setError("Couldn't use the camera. You can paste the link from your PC instead.");
      setPasting(true);
    }
  }

  return (
    <main className="flex flex-col gap-4 p-4">
      <header className="flex items-center gap-2 pt-2">
        {canGoBack && (
          <button className="rounded-full px-2 py-1 text-2xl text-zinc-300" aria-label="Back" onClick={onBack}>
            ‹
          </button>
        )}
        <h1 className="text-xl font-semibold">Pair with your PC</h1>
      </header>
      <ol className="flex flex-col gap-3 rounded-2xl bg-zinc-900 p-4 text-sm text-zinc-300 ring-1 ring-zinc-800">
        <li>
          <b className="text-zinc-100">1.</b> On your PC, open <b className="text-zinc-100">JuiceBox Trainer</b> and go
          to <b className="text-zinc-100">Cheats</b>.
        </li>
        <li>
          <b className="text-zinc-100">2.</b> Under <b className="text-zinc-100">Phone remote</b>, click{" "}
          <b className="text-zinc-100">Turn on</b>. A QR code shows.
        </li>
        <li>
          <b className="text-zinc-100">3.</b> With this phone on the same Wi-Fi as the PC, scan the code here.
        </li>
      </ol>
      <label className="flex flex-col gap-1 text-sm text-zinc-400">
        This phone&apos;s name (the PC shows it)
        <input
          className="rounded-xl bg-zinc-900 px-3 py-3 text-base text-zinc-100 ring-1 ring-zinc-700"
          value={phone}
          maxLength={40}
          onChange={(e) => setPhone(e.target.value)}
        />
      </label>
      <button
        className="btn-accent rounded-2xl px-4 py-4 text-base font-medium disabled:opacity-50"
        disabled={busy}
        onClick={() => void scan()}
      >
        {busy ? "Pairing…" : "Scan the QR code"}
      </button>
      {error && (
        <p role="alert" className="rounded-xl bg-red-950/60 px-3 py-2 text-sm text-red-200">
          {error}
        </p>
      )}
      {pasting ? (
        <form
          className="flex flex-col gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (link.trim()) void pair(link.trim());
          }}
        >
          <label className="flex flex-col gap-1 text-sm text-zinc-400">
            The QR code&apos;s link (starts with http://)
            <input
              className="rounded-xl bg-zinc-900 px-3 py-3 text-sm text-zinc-100 ring-1 ring-zinc-700"
              value={link}
              inputMode="url"
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck={false}
              onChange={(e) => setLink(e.target.value)}
            />
          </label>
          <button
            className="rounded-2xl bg-zinc-800 px-4 py-3 text-base disabled:opacity-50"
            disabled={busy || !link.trim()}
          >
            Pair
          </button>
        </form>
      ) : (
        <button className="text-sm text-zinc-400 underline" onClick={() => setPasting(true)}>
          No camera? Paste the link instead
        </button>
      )}
    </main>
  );
}
