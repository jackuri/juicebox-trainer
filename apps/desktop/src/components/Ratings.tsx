import { useState } from "react";
import { friendlyError } from "../lib/explain";
import { api, type DeviceCode, type Rating, toCommandError } from "../lib/ipc";
import { myRating, ratingWords, rememberRating, STAR_WORDS, starGlyphs } from "../lib/ratings";

const small = "rounded-lg bg-zinc-800 px-2.5 py-1 text-xs ring-1 ring-zinc-700 hover:bg-zinc-700 disabled:opacity-40";

/** How players rate a community pack (v1.29.0). */
export function Stars({ rating }: { rating?: Rating }) {
  if (!rating || rating.count === 0) return <span className="text-xs text-zinc-500">Not rated yet</span>;
  return (
    <span
      className="text-xs text-zinc-300"
      aria-label={`Rated ${rating.stars} of 3 stars by ${rating.count} player${rating.count === 1 ? "" : "s"}`}
      title={`${rating.votes[2]} said works great, ${rating.votes[1]} works with bugs, ${rating.votes[0]} broken`}
    >
      <span className="text-amber-300">{starGlyphs(rating.stars)}</span> {ratingWords(rating.stars)} · {rating.stars.toFixed(1)} from{" "}
      {rating.count} player{rating.count === 1 ? "" : "s"}
    </span>
  );
}

/**
 * Rate a community pack 1 to 3 stars, with the player's GitHub sign-in (the same one as sharing).
 * One rating per GitHub account and pack: rating again replaces it.
 */
export function RatePack({ id, name, rating }: { id: string; name: string; rating?: Rating }) {
  const [mine, setMine] = useState(() => myRating(id));
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [code, setCode] = useState<DeviceCode | null>(null);
  const [pending, setPending] = useState<1 | 2 | 3 | null>(null);

  if (!rating) {
    return <p className="text-xs text-zinc-500">Ratings for this pack open within an hour of it being added to the list.</p>;
  }
  const issue = rating.issue;

  async function rate(stars: 1 | 2 | 3) {
    setError(null);
    setBusy(true);
    try {
      await api.communityRate(issue, stars);
      rememberRating(id, stars);
      setMine(stars);
      setDone(true);
    } catch (e) {
      const err = toCommandError(e);
      if (err.kind === "signedOut") {
        setPending(stars);
        await signIn(stars);
      } else setError(friendlyError(err));
    } finally {
      setBusy(false);
    }
  }

  async function signIn(then: 1 | 2 | 3) {
    try {
      setCode(await api.githubSignIn());
      await api.githubSignInWait();
      setCode(null);
      setPending(null);
      await rate(then);
    } catch (e) {
      setCode(null);
      const err = toCommandError(e);
      if (err.kind !== "cancelled") setError(friendlyError(err));
    }
  }

  return (
    <div className="flex w-full flex-col gap-1.5" role="group" aria-label={`Rate ${name}`}>
      <div className="flex flex-wrap items-center gap-1.5 text-xs">
        <span className="text-zinc-400">{mine ? "Your rating:" : "Tried it? Rate it:"}</span>
        {([1, 2, 3] as const).map((s) => (
          <button
            key={s}
            className={`${small} ${mine === s ? "bg-amber-900/60 ring-amber-600" : ""}`}
            aria-pressed={mine === s}
            aria-label={`${s} star${s === 1 ? "" : "s"}: ${STAR_WORDS[s]}`}
            disabled={busy || code !== null}
            onClick={() => void rate(s)}
          >
            <span className="text-amber-300">{"★".repeat(s)}</span> {STAR_WORDS[s]}
          </button>
        ))}
      </div>
      {done && (
        <p className="text-xs text-emerald-300" role="status">
          Thanks! The list counts it within the hour. Rating again changes it.
        </p>
      )}
      {code && (
        <div className="flex flex-col gap-1 rounded-lg bg-zinc-950/70 p-2 text-xs text-zinc-300" role="status">
          <p>
            Rating uses a free GitHub account (the same sign-in as sharing; you sign in once). Your code:{" "}
            <span className="select-all font-mono text-base font-semibold tracking-widest text-white">{code.userCode}</span>
          </p>
          <div className="flex gap-2">
            <button
              className={small}
              onClick={() => {
                void navigator.clipboard?.writeText(code.userCode).catch(() => undefined);
                void api.githubOpenDevicePage().catch(() => undefined);
              }}
            >
              Copy the code and open GitHub
            </button>
            <button className={small} onClick={() => void api.githubSignInCancel().catch(() => undefined)}>
              Cancel
            </button>
          </div>
          <p className="text-zinc-500">Paste it on GitHub&apos;s page and click Authorize{pending ? `: your ${pending}-star rating is sent after that.` : "."}</p>
        </div>
      )}
      {error && (
        <p className="text-xs text-red-300" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
