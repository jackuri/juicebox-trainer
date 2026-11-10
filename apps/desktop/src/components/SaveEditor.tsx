import { useCallback, useEffect, useMemo, useState } from "react";
import { inspect } from "../lib/format";
import {
  api,
  type EditReport,
  type FileView,
  type SaveEdit,
  type SaveFile,
  toCommandError,
} from "../lib/ipc";

const button = "rounded bg-zinc-800 px-2 py-1 text-xs hover:bg-zinc-700 disabled:opacity-40";
const input =
  "min-w-0 rounded border border-zinc-700 bg-zinc-900 px-2 py-1 text-sm placeholder:text-zinc-600";

const HEX_PAGE = 256;
const SHOWN_FIELDS = 500;

function hex2(n: number): string {
  return n.toString(16).toUpperCase().padStart(2, "0");
}

interface Props {
  gameId: string;
  file: SaveFile;
  /** The game is running: edits can't be saved. */
  running: boolean;
  onSaved: (report: EditReport) => void;
  onClose: () => void;
}

/** Shows one save's values (or bytes) and saves edits. */
export function SaveEditor({ gameId, file, running, onSaved, onClose }: Props) {
  const [view, setView] = useState<FileView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [values, setValues] = useState<Record<string, string>>({});
  const [patches, setPatches] = useState<{ offset: number; hex: string }[]>([]);
  const [showBytes, setShowBytes] = useState(false);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    api.savesOpenFile(gameId, file.folder, file.path).then(
      (v) => {
        setView(v);
        setValues({});
        setPatches([]);
        setShowBytes(v.codec === "hex");
      },
      (e: unknown) => setError(toCommandError(e).message),
    );
  }, [gameId, file.folder, file.path]);

  useEffect(load, [load]);

  const shown = useMemo(() => {
    if (!view) return [];
    const q = search.trim().toLowerCase();
    return view.fields.filter(
      (f) => !q || f.path.toLowerCase().includes(q) || f.value.toLowerCase().includes(q),
    );
  }, [view, search]);

  const edits: SaveEdit[] = [
    ...Object.entries(values)
      .filter(([path, v]) => view?.fields.find((f) => f.path === path)?.value !== v)
      .map(([path, value]) => ({ kind: "set" as const, path, value })),
    ...patches.map((p) => ({ kind: "patch" as const, ...p })),
  ];

  async function save() {
    setError(null);
    setMessage(null);
    setBusy(true);
    try {
      const r = await api.savesEdit(gameId, file.folder, file.path, edits);
      setMessage(
        r.changed
          ? 'Saved. Your saves from just before are in the backup marked "Before an edit".'
          : "Nothing changed.",
      );
      onSaved(r);
      load();
    } catch (e) {
      setError(toCommandError(e).message);
    } finally {
      setBusy(false);
    }
  }

  const name = file.registry ? `${file.folder} (registry)` : file.path;

  return (
    <section
      className="flex flex-col gap-3 rounded-lg border border-zinc-800 p-3"
      aria-label="Save editor"
    >
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="min-w-0 truncate font-mono text-sm">{name}</h3>
        {view && (
          <span className="text-xs text-zinc-400">
            {[view.format, ...view.layers].join(" · ")}
          </span>
        )}
        <button className={`${button} ml-auto`} onClick={onClose}>
          Close
        </button>
      </div>
      {view?.note && <p className="text-xs text-amber-300">{view.note}</p>}
      {running && (
        <p className="text-xs text-emerald-300" role="status">
          The game is running. You can look, but close it before saving changes (it would overwrite
          them).
        </p>
      )}
      {error && (
        <p className="rounded bg-red-950/40 px-2 py-1 text-sm text-red-300" role="alert">
          {error}
        </p>
      )}
      {message && (
        <p className="text-sm text-emerald-300" role="status">
          {message}
        </p>
      )}

      {view && view.fields.length > 0 && (
        <>
          <input
            aria-label="Search values"
            className={input}
            placeholder={`Search ${view.fields.length} values, e.g. gold`}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <div className="max-h-96 overflow-y-auto">
            <table className="w-full text-left text-sm">
              <tbody>
                {shown.slice(0, SHOWN_FIELDS).map((f) => (
                  <tr key={f.path} className="border-t border-zinc-800">
                    <td className="py-1 pr-2 font-mono text-xs">{f.path}</td>
                    <td className="py-1 pr-2">
                      {f.editable ? (
                        <input
                          aria-label={`Value of ${f.path}`}
                          className={`${input} w-full`}
                          value={values[f.path] ?? f.value}
                          onChange={(e) => setValues((v) => ({ ...v, [f.path]: e.target.value }))}
                        />
                      ) : (
                        <span className="text-xs text-zinc-500">{f.value}</span>
                      )}
                    </td>
                    <td className="py-1 text-[11px] text-zinc-500">{f.note ?? f.kind}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {shown.length > SHOWN_FIELDS && (
              <p className="text-xs text-zinc-500">
                Showing {SHOWN_FIELDS} of {shown.length}. Search to narrow it down.
              </p>
            )}
            {view.truncated && (
              <p className="text-xs text-zinc-500">This save has more values than can be listed.</p>
            )}
          </div>
        </>
      )}

      {view && !view.registry && (
        <div className="flex flex-col gap-2">
          {view.codec !== "hex" && (
            <button className={`${button} self-start`} onClick={() => setShowBytes((s) => !s)}>
              {showBytes ? "Hide bytes" : "Show bytes"}
            </button>
          )}
          {showBytes && (
            <HexView
              gameId={gameId}
              file={file}
              size={view.bodyLen}
              onPatch={(p) => setPatches((list) => [...list, p])}
            />
          )}
          {patches.length > 0 && (
            <p className="text-xs text-zinc-400">
              Byte changes waiting:{" "}
              {patches.map((p) => `0x${p.offset.toString(16).toUpperCase()}: ${p.hex}`).join(", ")}
            </p>
          )}
        </div>
      )}

      <div className="flex items-center gap-2">
        <button
          className="rounded btn-accent px-3 py-1 text-sm font-semibold disabled:opacity-40"
          disabled={busy || running || edits.length === 0}
          onClick={() => void save()}
        >
          Save changes{edits.length > 0 ? ` (${edits.length})` : ""}
        </button>
        <button
          className={button}
          disabled={edits.length === 0}
          onClick={() => {
            setValues({});
            setPatches([]);
          }}
        >
          Undo changes
        </button>
        <span className="text-xs text-zinc-500">Your saves are backed up before anything is written.</span>
      </div>
    </section>
  );
}

interface HexProps {
  gameId: string;
  file: SaveFile;
  size: number;
  onPatch: (p: { offset: number; hex: string }) => void;
}

function HexView({ gameId, file, size, onPatch }: HexProps) {
  const [offset, setOffset] = useState(0);
  const [bytes, setBytes] = useState<number[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [patchHex, setPatchHex] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.savesHex(gameId, file.folder, file.path, offset, HEX_PAGE + 8).then(setBytes, (e: unknown) =>
      setError(toCommandError(e).message),
    );
  }, [gameId, file.folder, file.path, offset]);

  const rows = [];
  for (let r = 0; r < Math.min(bytes.length, HEX_PAGE); r += 16) rows.push(r);
  const sel = selected === null ? [] : bytes.slice(selected - offset, selected - offset + 8);

  function addPatch() {
    const clean = patchHex.replace(/\s+/g, "");
    if (selected === null || !/^([0-9a-fA-F]{2})+$/.test(clean)) {
      setError("Pick a byte, then type whole hex bytes like E8 03 00 00.");
      return;
    }
    setError(null);
    onPatch({ offset: selected, hex: clean.toUpperCase() });
    setPatchHex("");
  }

  return (
    <div className="flex flex-col gap-2" aria-label="Bytes">
      <div className="flex items-center gap-2 text-xs">
        <button className={button} disabled={offset === 0} onClick={() => setOffset((o) => Math.max(0, o - HEX_PAGE))}>
          Previous
        </button>
        <button
          className={button}
          disabled={offset + HEX_PAGE >= size}
          onClick={() => setOffset((o) => o + HEX_PAGE)}
        >
          Next
        </button>
        <span className="text-zinc-500">
          Bytes 0x{offset.toString(16).toUpperCase()}–0x
          {Math.min(size, offset + HEX_PAGE).toString(16).toUpperCase()} of {size}
        </span>
      </div>
      <pre className="overflow-x-auto rounded bg-zinc-950 p-2 font-mono text-xs leading-5">
        {rows.map((r) => (
          <div key={r}>
            <span className="text-zinc-600">{(offset + r).toString(16).toUpperCase().padStart(8, "0")} </span>
            {bytes.slice(r, r + 16).map((b, i) => {
              const at = offset + r + i;
              return (
                <button
                  key={i}
                  className={`px-0.5 ${at === selected ? "bg-emerald-800 text-bright" : "hover:bg-zinc-800"}`}
                  aria-label={`Byte at 0x${at.toString(16).toUpperCase()}`}
                  onClick={() => setSelected(at)}
                >
                  {hex2(b)}
                </button>
              );
            })}
          </div>
        ))}
      </pre>
      {selected !== null && (
        <div className="flex flex-wrap items-start gap-4 text-xs">
          <div>
            <p className="text-zinc-400">At 0x{selected.toString(16).toUpperCase()} (unsigned / signed):</p>
            <ul aria-label="Number at selected byte">
              {inspect(sel).map((n) => (
                <li key={n.label}>
                  <span className="text-zinc-500">{n.label}:</span> {n.value}
                </li>
              ))}
            </ul>
          </div>
          <div className="flex items-center gap-1">
            <input
              aria-label="New bytes"
              className={`${input} w-40 font-mono text-xs`}
              placeholder="E8 03 00 00"
              value={patchHex}
              onChange={(e) => setPatchHex(e.target.value)}
            />
            <button className={button} onClick={addPatch}>
              Change bytes here
            </button>
          </div>
        </div>
      )}
      {error && <p className="text-xs text-red-300">{error}</p>}
    </div>
  );
}
