import { fileSize } from "../lib/format";
import type { FileChange, SaveDiff } from "../lib/ipc";

const CHANGE_TEXT: Record<FileChange["change"], string> = {
  added: "new file",
  removed: "file removed",
  changed: "changed",
};

const LINE_CLASS = {
  same: "text-zinc-500",
  removed: "bg-red-950/50 text-red-200",
  added: "bg-emerald-950/50 text-emerald-200",
} as const;

const LINE_MARK = { same: " ", removed: "-", added: "+" } as const;

function Detail({ file }: { file: FileChange }) {
  const d = file.detail;
  if (!d) return null;
  if (d.kind === "tooLarge") {
    return <p className="text-xs text-zinc-500">Too big to show what changed.</p>;
  }
  if (d.kind === "text") {
    return (
      <div className="flex flex-col gap-2">
        {d.hunks.map((h, i) => (
          <pre
            key={i}
            className="overflow-x-auto rounded bg-zinc-950 p-2 font-mono text-xs leading-5"
          >
            <span className="text-zinc-600">line {h.newLine}</span>
            {"\n"}
            {h.lines.map((l, j) => (
              <span key={j} className={`block ${LINE_CLASS[l.tag]}`}>
                {LINE_MARK[l.tag]} {l.text}
              </span>
            ))}
          </pre>
        ))}
        {d.more > 0 && <p className="text-xs text-zinc-500">…and {d.more} more changes.</p>}
      </div>
    );
  }
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-left font-mono text-xs">
        <thead className="text-zinc-500">
          <tr>
            <th className="py-1 pr-3 font-normal">Offset</th>
            <th className="py-1 pr-3 font-normal">Before</th>
            <th className="py-1 pr-3 font-normal">After</th>
            <th className="py-1 font-normal">As a number</th>
          </tr>
        </thead>
        <tbody>
          {d.ranges.map((r) => (
            <tr key={r.offset} className="border-t border-zinc-800">
              <td className="py-1 pr-3">0x{r.offset.toString(16).toUpperCase()}</td>
              <td className="py-1 pr-3 text-red-300">{r.old}</td>
              <td className="py-1 pr-3 text-emerald-300">{r.new}</td>
              <td className="py-1">
                {r.oldI32 !== null && r.newI32 !== null ? `${r.oldI32} → ${r.newI32}` : ""}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {d.more > 0 && <p className="text-xs text-zinc-500">…and {d.more} more changes.</p>}
    </div>
  );
}

interface Props {
  diff: SaveDiff;
  title: string;
  onClose: () => void;
}

/** What changed between two backups (or a backup and the saves now). */
export function SaveDiffView({ diff, title, onClose }: Props) {
  return (
    <section className="flex flex-col gap-3 rounded-md border border-zinc-800 p-3" aria-label="Comparison">
      <div className="flex items-center gap-2">
        <h3 className="text-sm font-medium">{title}</h3>
        <button
          className="ml-auto rounded bg-zinc-800 px-2 py-0.5 text-xs hover:bg-zinc-700"
          onClick={onClose}
        >
          Close
        </button>
      </div>
      {diff.files.length === 0 ? (
        <p className="text-sm text-zinc-400">No differences: the save files are the same.</p>
      ) : (
        <ul className="flex flex-col gap-3">
          {diff.files.map((f) => (
            <li key={`${f.folder}|${f.path}`} className="flex flex-col gap-1">
              <p className="text-sm">
                <span className="font-mono">{f.path}</span>{" "}
                <span className="text-xs text-zinc-400">
                  {CHANGE_TEXT[f.change]}
                  {f.oldSize !== null && f.newSize !== null && f.oldSize !== f.newSize
                    ? ` (${fileSize(f.oldSize)} → ${fileSize(f.newSize)})`
                    : ""}
                </span>
              </p>
              <Detail file={f} />
            </li>
          ))}
        </ul>
      )}
      {diff.unchanged > 0 && (
        <p className="text-xs text-zinc-500">
          {diff.unchanged} other file{diff.unchanged === 1 ? "" : "s"} unchanged.
        </p>
      )}
    </section>
  );
}
