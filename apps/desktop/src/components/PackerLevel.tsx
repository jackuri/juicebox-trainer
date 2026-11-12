import { useCallback, useEffect, useState } from "react";
import { friendlyError } from "../lib/explain";
import { api, type DeviceCode, type GithubStatus, type Player, toCommandError } from "../lib/ipc";
import {
  checkedIn,
  checkInOn,
  findPlayer,
  levelUp,
  nextTitle,
  rememberCheckIn,
  setCheckInOn,
  standing,
  STREAK_DAYS,
  STREAK_XP,
  streakReward,
  streakWithToday,
  TITLES,
  titleFor,
  utcToday,
  XP_PER_DAY,
  XP_PER_PACK,
  XP_PER_RATING,
} from "../lib/levels";

const small = "rounded-lg bg-zinc-800 px-3 py-1.5 text-xs ring-1 ring-zinc-700 hover:bg-zinc-700 disabled:opacity-40";
/** Look again for new XP this often (the list itself changes hourly). */
const REFRESH_MS = 30 * 60 * 1000;
/** Today's check-in page opens just after midnight UTC: try again this often until it's there. */
const CHECK_IN_RETRY_MS = 10 * 60 * 1000;

/** The XP bar: how far into this level. */
function XpBar({ into, need, max, big = false }: { into: number; need: number; max: boolean; big?: boolean }) {
  const pct = max ? 100 : Math.min(100, Math.round((into / need) * 100));
  return (
    <div
      className={`relative w-full overflow-hidden rounded-md bg-zinc-950 ring-1 ring-zinc-700 ${big ? "h-6" : "h-2"}`}
      role="progressbar"
      aria-label="XP to the next level"
      aria-valuemin={0}
      aria-valuemax={max ? into : need}
      aria-valuenow={into}
    >
      <div className="h-full bg-amber-400" style={{ width: `${pct}%` }} />
      {big && (
        <span className="absolute inset-0 flex items-center px-2 font-mono text-xs font-bold text-zinc-950 mix-blend-normal">
          <span className="rounded bg-amber-300/80 px-1">{max ? `XP ${into} · top level` : `XP ${into}/${need}`}</span>
        </span>
      )}
    </div>
  );
}

/** Five dots for the days of the current 5-day run ("●●●○○"). */
function StreakDots({ streak, light = false }: { streak: number; light?: boolean }) {
  const filled = streak <= 0 ? 0 : ((streak - 1) % STREAK_DAYS) + 1;
  return (
    <span className="flex gap-1" role="img" aria-label={`${filled} of ${STREAK_DAYS} days in a row`}>
      {Array.from({ length: STREAK_DAYS }, (_, i) => (
        <span
          key={i}
          className={`h-3 w-3 rounded-sm ring-2 ${
            i < filled ? "bg-amber-400 ring-amber-700" : light ? "bg-teal-950/30 ring-teal-900/60" : "bg-zinc-800 ring-zinc-700"
          }`}
        />
      ))}
    </span>
  );
}

/** The round level badge ("LVL 12"). */
function LevelBadge({ level, big = false }: { level: number; big?: boolean }) {
  return (
    <span
      className={`flex shrink-0 flex-col items-center justify-center rounded-full bg-teal-900 font-mono font-bold leading-none text-white ring-2 ring-teal-500 ${
        big ? "h-16 w-16 text-2xl" : "h-9 w-9 text-sm"
      }`}
      aria-hidden="true"
    >
      <span className={big ? "text-[10px]" : "text-[7px]"}>LVL</span>
      {level}
    </span>
  );
}

/**
 * The player's packer level in the sidebar (v1.32.0): level, title and XP bar from the community
 * list's XP robot, for their GitHub name. Opens the level panel; celebrates a level up.
 */
export function PackerBadge() {
  const [github, setGithub] = useState<GithubStatus | null>(null);
  const [players, setPlayers] = useState<Player[] | null>(null);
  const [open, setOpen] = useState(false);
  const [celebrate, setCelebrate] = useState<number | null>(null);
  /** Daily check-in (v1.33.0): turned on by the player; today's, once done; a tick to try again. */
  const [checkIn, setCheckIn] = useState(checkInOn);
  const [daily, setDaily] = useState<string | null>(null);
  const [tries, setTries] = useState(0);

  const load = useCallback((refresh: boolean) => {
    api.githubStatus().then(setGithub, () => undefined);
    api.communityXp(refresh).then(
      (list) => setPlayers(Array.isArray(list) ? list : []),
      () => undefined,
    );
  }, []);
  useEffect(() => {
    load(false);
    const timer = window.setInterval(() => {
      load(false);
      setTries((n) => n + 1);
    }, REFRESH_MS);
    return () => window.clearInterval(timer);
  }, [load]);

  const login = github?.signedIn ? github.login : null;
  const me = players ? findPlayer(players, login) : null;
  const xp = me?.xp ?? 0;
  const s = standing(xp);
  // A level up since this PC last saw this player's level.
  useEffect(() => {
    if (!login || !players) return;
    const up = levelUp(login, s.level);
    if (up) window.setTimeout(() => setCelebrate(up), 0);
  }, [login, players, s.level]);

  // Check in once a day (UTC), when the player turned it on and is signed in to GitHub.
  useEffect(() => {
    if (!login || !checkIn) return;
    const today = utcToday();
    if (checkedIn(login, today)) return;
    let live = true;
    let retry: number | undefined;
    api.communityCheckIn().then(
      (r) => {
        if (!live) return;
        if (r.status === "notYet") {
          retry = window.setTimeout(() => setTries((n) => n + 1), CHECK_IN_RETRY_MS);
          return;
        }
        rememberCheckIn(login, r.day);
        if (r.status === "done") setDaily(r.day);
      },
      () => undefined,
    );
    return () => {
      live = false;
      window.clearTimeout(retry);
    };
  }, [login, checkIn, tries]);
  const streak = daily ? streakWithToday(me, daily) : 0;

  return (
    <>
      <button
        className="mb-2 flex w-full items-center gap-2 rounded-xl bg-zinc-900/80 px-2.5 py-2 text-left ring-1 ring-zinc-800 hover:bg-zinc-800/80"
        aria-label={login ? `Packer level ${s.level}, ${s.title}` : "Packer level"}
        onClick={() => setOpen(true)}
      >
        <LevelBadge level={login ? s.level : 1} />
        <span className="flex min-w-0 flex-1 flex-col gap-1">
          <span className="truncate text-xs font-semibold text-zinc-100">{login ? s.title : "Packer level"}</span>
          {login ? (
            <XpBar into={s.into} need={s.need} max={s.max} />
          ) : (
            <span className="text-[11px] text-zinc-500">Share and rate packs to level up</span>
          )}
        </span>
      </button>
      {open && (
        <PackerPanel
          github={github}
          players={players}
          checkIn={checkIn}
          onCheckIn={(on) => {
            setCheckInOn(on);
            setCheckIn(on);
          }}
          onClose={() => setOpen(false)}
          onChanged={() => load(true)}
        />
      )}
      {daily && celebrate === null && <DailyCheckIn streak={streak} onClose={() => setDaily(null)} />}
      {celebrate !== null && <LevelUp level={celebrate} onClose={() => setCelebrate(null)} />}
    </>
  );
}

/** The level panel: your level and title, how to earn XP, the titles and the leaderboard. */
export function PackerPanel({
  github,
  players,
  checkIn = false,
  onCheckIn,
  onClose,
  onChanged,
}: {
  github: GithubStatus | null;
  players: Player[] | null;
  /** The daily check-in is on (v1.33.0), and turning it on or off. */
  checkIn?: boolean;
  onCheckIn?: (on: boolean) => void;
  onClose: () => void;
  onChanged: () => void;
}) {
  const login = github?.signedIn ? github.login : null;
  const me = players ? findPlayer(players, login) : null;
  const s = standing(me?.xp ?? 0);
  const next = nextTitle(s.level);
  const rank = me && players ? players.indexOf(me) + 1 : null;
  const [code, setCode] = useState<DeviceCode | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function signIn() {
    setError(null);
    try {
      setCode(await api.githubSignIn());
      await api.githubSignInWait();
      setCode(null);
      onChanged();
    } catch (e) {
      setCode(null);
      const err = toCommandError(e);
      if (err.kind !== "cancelled") setError(friendlyError(err));
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4" onClick={onClose}>
      <section
        className="flex max-h-full w-full max-w-xl flex-col gap-4 overflow-y-auto rounded-2xl bg-zinc-900 p-5 ring-1 ring-teal-700"
        role="dialog"
        aria-modal="true"
        aria-label="Your packer level"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-3">
          <LevelBadge level={s.level} big />
          <div className="flex min-w-0 flex-1 flex-col gap-1.5">
            <h2 className="text-lg font-semibold">{login ? s.title : "Your packer level"}</h2>
            {login ? (
              <XpBar into={s.into} need={s.need} max={s.max} big />
            ) : (
              <p className="text-sm text-zinc-400">Levels go with your GitHub name, the same sign-in as sharing and rating.</p>
            )}
          </div>
          <button className={`${small} self-start`} onClick={onClose}>
            Close
          </button>
        </div>

        {login ? (
          <p className="text-sm text-zinc-300">
            {login}: level {s.level}, {me?.xp ?? 0} XP
            {me ? ` (${me.packs} pack${me.packs === 1 ? "" : "s"} accepted, ${me.ratings} rated)` : ""}
            {rank ? ` · #${rank} on the leaderboard` : ""}.{" "}
            {next ? `${next.title} at level ${next.level}.` : "You're at the top!"}
          </p>
        ) : (
          <div className="flex flex-col gap-2 text-sm">
            {!github?.configured ? (
              <p className="text-zinc-400">This version of JuiceBox Trainer can&apos;t sign in to GitHub.</p>
            ) : code ? (
              <div className="flex flex-col gap-1 rounded-lg bg-zinc-950/70 p-2 text-xs text-zinc-300" role="status">
                <p>
                  Your code: <span className="select-all font-mono text-base font-semibold tracking-widest text-white">{code.userCode}</span>
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
                <p className="text-zinc-500">Paste it on GitHub&apos;s page and click Authorize.</p>
              </div>
            ) : (
              <button className="btn-accent self-start rounded-lg px-4 py-1.5 text-sm font-semibold" onClick={() => void signIn()}>
                Sign in with GitHub (free)
              </button>
            )}
            {error && (
              <p className="text-xs text-red-300" role="alert">
                {error}
              </p>
            )}
          </div>
        )}

        <div className="flex flex-col gap-1 text-sm">
          <h3 className="font-semibold">How to earn XP</h3>
          <ul className="list-disc pl-5 text-zinc-300">
            <li>
              <b>{XP_PER_PACK} XP</b> for each pack or web cheats you share that&apos;s accepted into the community list (Packs → Share with
              everyone).
            </li>
            <li>
              <b>{XP_PER_RATING} XP</b> for each community pack you rate, your own too (once per pack).
            </li>
            <li>
              <b>{XP_PER_DAY} XP</b> for each day you open JuiceBox Trainer with the daily check-in on, and <b>{STREAK_XP} XP</b> more
              for every {STREAK_DAYS} days in a row.
            </li>
          </ul>
          <p className="text-xs text-zinc-500">
            GitHub counts it, every hour: new XP shows up within the hour. Level 2 takes 10 XP, and each next level 10 XP more than the
            one before.
          </p>
        </div>

        <div className="flex flex-col gap-1.5 text-sm" role="group" aria-label="Daily check-in">
          <h3 className="font-semibold">Daily check-in</h3>
          {login ? (
            <>
              <label className="flex items-start gap-2">
                <input type="checkbox" className="mt-1" checked={checkIn} onChange={(e) => onCheckIn?.(e.target.checked)} />
                <span>
                  Check me in every day I open JuiceBox Trainer ({XP_PER_DAY} XP a day, {STREAK_XP} XP more for every {STREAK_DAYS} days in a
                  row).
                </span>
              </label>
              <p className="text-xs text-zinc-500">
                It puts a 👍 with your GitHub name on that day&apos;s check-in page of the community list, which anyone can see. Days go by
                world time (UTC).
              </p>
              {checkIn && (
                <div className="flex items-center gap-2 text-xs text-zinc-300">
                  <StreakDots streak={checkedIn(login, utcToday()) ? streakWithToday(me, utcToday()) : (me?.streak ?? 0)} />
                  <span>
                    {checkedIn(login, utcToday()) ? "Checked in today." : "Not checked in yet today."}
                    {me?.days ? ` ${me.days} day${me.days === 1 ? "" : "s"} so far.` : ""}
                  </span>
                </div>
              )}
            </>
          ) : (
            <p className="text-xs text-zinc-400">Sign in with GitHub (above) to check in every day you open JuiceBox Trainer.</p>
          )}
        </div>

        <div className="flex flex-col gap-1 text-sm">
          <h3 className="font-semibold">Titles</h3>
          <ul className="grid grid-cols-2 gap-x-4 gap-y-0.5 text-xs" aria-label="Titles">
            {TITLES.map(([at, title]) => (
              <li key={title} className={title === s.title && login ? "font-semibold text-amber-300" : s.level >= at && login ? "text-zinc-300" : "text-zinc-500"}>
                Level {at}: {title}
              </li>
            ))}
          </ul>
        </div>

        <div className="flex flex-col gap-1 text-sm">
          <div className="flex items-center gap-2">
            <h3 className="font-semibold">Leaderboard</h3>
            <button className={`${small} ml-auto`} onClick={onChanged}>
              Refresh
            </button>
          </div>
          {players === null ? (
            <p className="text-xs text-zinc-500">Couldn&apos;t load the leaderboard (offline?).</p>
          ) : players.length === 0 ? (
            <p className="text-xs text-zinc-500">No one has XP yet. Be the first!</p>
          ) : (
            <ol className="flex flex-col gap-0.5 text-xs" aria-label="Leaderboard">
              {players.slice(0, 20).map((p, i) => {
                const ps = standing(p.xp);
                const mine = me?.login === p.login;
                return (
                  <li key={p.login} className={`flex items-center gap-2 rounded px-2 py-1 ${mine ? "bg-amber-900/40 text-amber-100" : "bg-zinc-950/50"}`}>
                    <span className="w-6 text-right font-mono text-zinc-500">{i + 1}</span>
                    <span className="min-w-0 flex-1 truncate font-medium">{p.login}</span>
                    <span className="text-zinc-400">{ps.title}</span>
                    <span className="w-14 text-right font-mono">LVL {ps.level}</span>
                    <span className="w-16 text-right font-mono text-zinc-500">{p.xp} XP</span>
                  </li>
                );
              })}
            </ol>
          )}
        </div>
      </section>
    </div>
  );
}

/** The daily check-in card: +2 XP, the 5-day run, and the streak reward on every 5th day. */
export function DailyCheckIn({ streak, onClose }: { streak: number; onClose: () => void }) {
  const reward = streakReward(streak);
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4" onClick={onClose}>
      <section
        className="relative flex w-80 flex-col items-center gap-3 rounded-xl bg-teal-800 p-4 text-center shadow-2xl ring-4 ring-teal-950"
        role="dialog"
        aria-modal="true"
        aria-label="Daily check-in"
        onClick={(e) => e.stopPropagation()}
      >
        {reward && (
          <>
            <span className="pointer-events-none absolute -left-6 top-6 text-2xl text-amber-300" aria-hidden="true">
              ★
            </span>
            <span className="pointer-events-none absolute -right-5 top-12 text-xl text-amber-300" aria-hidden="true">
              ★
            </span>
          </>
        )}
        <div className="flex w-full flex-col items-center gap-2 rounded-md bg-[#a8c4a8] px-3 py-4 font-mono text-teal-950 ring-4 ring-teal-950">
          <p className="text-lg font-black tracking-wider">DAILY CHECK-IN</p>
          <p className="text-3xl font-black">+{XP_PER_DAY} XP</p>
          <StreakDots streak={streak} light />
          <p className="text-sm font-bold">
            Day {streak} in a row{reward ? "!" : ""}
          </p>
        </div>
        {reward ? (
          <p className="w-full rounded bg-amber-400 px-2 py-1 font-mono text-sm font-black uppercase tracking-wider text-zinc-950">
            {STREAK_DAYS}-day streak: +{STREAK_XP} XP!
          </p>
        ) : (
          <p className="w-full rounded bg-zinc-950 px-2 py-1 font-mono text-xs font-bold uppercase tracking-wider text-white">
            {STREAK_DAYS - (streak % STREAK_DAYS)} more day{STREAK_DAYS - (streak % STREAK_DAYS) === 1 ? "" : "s"} for +{STREAK_XP} XP
          </p>
        )}
        <p className="text-[11px] text-teal-100">Your XP shows up within the hour.</p>
        <button className="rounded-lg bg-red-600 px-6 py-2 font-mono text-lg font-bold text-white ring-2 ring-red-900 hover:bg-red-500" onClick={onClose} autoFocus>
          OK
        </button>
      </section>
    </div>
  );
}

/** "LEVEL UP!" */
export function LevelUp({ level, onClose }: { level: number; onClose: () => void }) {
  const title = titleFor(level);
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4" onClick={onClose}>
      <section
        className="relative flex w-80 flex-col items-center gap-3 rounded-xl bg-teal-800 p-4 text-center shadow-2xl ring-4 ring-teal-950"
        role="dialog"
        aria-modal="true"
        aria-label="Level up"
        onClick={(e) => e.stopPropagation()}
      >
        <span className="pointer-events-none absolute -left-6 top-6 text-2xl text-amber-300" aria-hidden="true">
          ★
        </span>
        <span className="pointer-events-none absolute -right-5 top-12 text-xl text-amber-300" aria-hidden="true">
          ★
        </span>
        <span className="pointer-events-none absolute -left-4 bottom-10 text-xl text-amber-300" aria-hidden="true">
          ★
        </span>
        <div className="w-full rounded-md bg-[#a8c4a8] px-3 py-4 font-mono text-teal-950 ring-4 ring-teal-950">
          <p className="text-2xl" aria-hidden="true">
            ^_~
          </p>
          <p className="text-2xl font-black tracking-wider">LEVEL UP!</p>
        </div>
        <p className="w-full rounded bg-zinc-950 px-2 py-1 font-mono text-sm font-bold uppercase tracking-widest text-white">{title}</p>
        <div className="flex items-center gap-4">
          <LevelBadge level={level} big />
          <button className="rounded-lg bg-red-600 px-5 py-2 font-mono text-lg font-bold text-white ring-2 ring-red-900 hover:bg-red-500" onClick={onClose} autoFocus>
            OK
          </button>
        </div>
      </section>
    </div>
  );
}
