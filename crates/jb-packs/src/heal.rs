//! Repairing a pack after a game update (PLAN.md §5.4 "self-healing").
//!
//! For every cheat found by a byte signature (AOB) that no longer matches
//! exactly once, look for the closest match in the new build:
//! - patch cheats: every place that holds the cheat's `original` bytes is a
//!   candidate; candidates are scored by how many of the old signature's
//!   fixed bytes still match around them;
//! - value cheats with an AOB address: places matching the longest run of
//!   fixed bytes in the old signature are candidates, scored the same way.
//!
//! Only a single clear winner (at least 60% of the fixed bytes, and better
//! than every other candidate) is accepted. Its new signature is made with
//! the signature maker, so it matches exactly once. Nothing is written to the
//! game; the result is a new pack to try.
//!
//! Static addresses (`base`) can't be repaired this way: the value moved, and
//! a new pointer scan is needed (the AI helper can do that).

use jb_core::{AttachedProcess, Module, Pattern};
use serde::Serialize;

use crate::schema::{Build, Impl, Pack};

/// A repair is accepted only above this share of the old fixed bytes.
const MIN_SCORE: f64 = 0.6;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fix {
    pub cheat: String,
    pub old_aob: String,
    pub new_aob: String,
    /// The repaired site, `game.exe+0x1234`.
    pub at: String,
    /// Old fixed bytes that still matched, out of all of them.
    pub matched: usize,
    pub of: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealReport {
    /// Cheats that already work on this build.
    pub ok: Vec<String>,
    pub fixed: Vec<Fix>,
    /// Cheat id → why it couldn't be repaired.
    pub broken: Vec<(String, String)>,
    /// The pack with the fixes, a bumped `packVersion` and this build's hash.
    pub pack: Pack,
}

/// `1.2.3` → `1.2.4`; anything else gets `.1` added.
fn bump(v: &str) -> String {
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() == 3
        && let Ok(n) = parts[2].parse::<u64>()
    {
        return format!("{}.{}.{}", parts[0], parts[1], n + 1);
    }
    format!("{v}.1")
}

struct Image {
    module: Module,
    /// (start address, bytes) for each readable piece.
    pieces: Vec<(u64, Vec<u8>)>,
}

impl Image {
    fn read(process: &AttachedProcess, module: Module) -> jb_core::Result<Self> {
        let pieces = process.module_image(&module)?;
        Ok(Self { module, pieces })
    }

    /// Addresses where `needle` occurs.
    fn find(&self, needle: &[u8]) -> Vec<u64> {
        let mut out = Vec::new();
        if needle.is_empty() {
            return out;
        }
        for (start, bytes) in &self.pieces {
            for (i, w) in bytes.windows(needle.len()).enumerate() {
                if w == needle {
                    out.push(start + i as u64);
                }
            }
        }
        out
    }

    fn byte(&self, address: u64) -> Option<u8> {
        self.pieces.iter().find_map(|(start, bytes)| {
            let i = address.checked_sub(*start)? as usize;
            bytes.get(i).copied()
        })
    }

    /// How many fixed bytes of `pattern` match if it starts at `start`.
    fn score(&self, pattern: &Pattern, start: u64) -> usize {
        pattern
            .bytes()
            .iter()
            .enumerate()
            .filter(|(i, b)| b.is_some_and(|b| self.byte(start + *i as u64) == Some(b)))
            .count()
    }

    fn label(&self, address: u64) -> String {
        format!("{}+{:#X}", self.module.name, address - self.module.base)
    }
}

/// The best candidate start, if it is a clear, good enough winner.
fn best(image: &Image, pattern: &Pattern, starts: Vec<u64>) -> Result<(u64, usize), String> {
    let fixed = pattern.fixed_len();
    let mut scored: Vec<(u64, usize)> = starts
        .into_iter()
        .map(|s| (s, image.score(pattern, s)))
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.dedup_by_key(|c| c.0);
    let Some(&(start, score)) = scored.first() else {
        return Err("the code it changes isn't in this version of the game".into());
    };
    if (score as f64) < MIN_SCORE * fixed as f64 {
        return Err(format!(
            "the closest match is too different ({score} of {fixed} bytes)"
        ));
    }
    if scored.get(1).is_some_and(|c| c.1 == score) {
        return Err("more than one place matches equally well".into());
    }
    Ok((start, score))
}

fn unique(process: &AttachedProcess, module: &Module, aob: &str) -> bool {
    Pattern::parse(aob)
        .and_then(|p| process.aob_scan(module, &p, 2))
        .is_ok_and(|h| h.len() == 1)
}

/// Tries to repair every AOB-based cheat of `pack` for the attached build.
pub fn heal(process: &AttachedProcess, pack: &Pack) -> jb_core::Result<HealReport> {
    let mut out = pack.clone();
    let mut ok = Vec::new();
    let mut fixed = Vec::new();
    let mut broken = Vec::new();
    for cheat in &mut out.cheats {
        let id = cheat.id.clone();
        let (module_name, aob, anchor) = match &cheat.implementation {
            Impl::Patch(p) => (
                p.module.clone(),
                p.aob.clone(),
                Some((p.offset, p.original.clone())),
            ),
            Impl::Value(v) => match &v.address.aob {
                Some(aob) => (v.address.module.clone(), aob.clone(), None),
                None => {
                    ok.push(id); // static addresses are checked by the trainer itself
                    continue;
                }
            },
        };
        let module = match process.module(module_name.as_deref().unwrap_or(&pack.game.exe)) {
            Ok(m) => m,
            Err(e) => {
                broken.push((id, e.to_string()));
                continue;
            }
        };
        if unique(process, &module, &aob) {
            ok.push(id);
            continue;
        }
        let Ok(pattern) = Pattern::parse(&aob) else {
            broken.push((id, "its signature isn't valid".into()));
            continue;
        };
        let image = Image::read(process, module)?;
        // Candidate pattern starts.
        let starts: Vec<u64> = match &anchor {
            Some((offset, original)) => {
                let Ok(orig) = Pattern::parse_exact(original) else {
                    broken.push((id, "its original bytes aren't valid".into()));
                    continue;
                };
                image
                    .find(&orig)
                    .into_iter()
                    .filter_map(|a| a.checked_add_signed(-offset))
                    .collect()
            }
            None => {
                // Every run of 4 fixed bytes anchors a search, so a changed
                // byte anywhere in the old signature doesn't hide the site.
                let bytes = pattern.bytes();
                let mut starts = Vec::new();
                for i in 0..bytes.len().saturating_sub(3) {
                    let window: Option<Vec<u8>> = bytes[i..i + 4].iter().copied().collect();
                    if let Some(w) = window {
                        starts.extend(
                            image
                                .find(&w)
                                .into_iter()
                                .filter_map(|a| a.checked_sub(i as u64)),
                        );
                    }
                }
                if starts.is_empty() && pattern.fixed_len() < 4 {
                    broken.push((
                        id,
                        "its signature has too few fixed bytes to search for".into(),
                    ));
                    continue;
                }
                starts
            }
        };
        let (start, score) = match best(&image, &pattern, starts) {
            Ok(b) => b,
            Err(why) => {
                broken.push((id, why));
                continue;
            }
        };
        // A fresh signature for the repaired site: for a patch, starting at
        // the patched instruction; for a value, where the old one started.
        let site = match &anchor {
            Some((offset, _)) => start.checked_add_signed(*offset).unwrap_or(start),
            None => start,
        };
        let sig = match jb_core::signature::make(process, site) {
            Ok(s) => s,
            Err(e) => {
                broken.push((id, e.to_string()));
                continue;
            }
        };
        match &mut cheat.implementation {
            Impl::Patch(p) => {
                p.aob = sig.pattern.clone();
                p.offset = 0;
            }
            Impl::Value(v) => v.address.aob = Some(sig.pattern.clone()),
        }
        fixed.push(Fix {
            cheat: id,
            old_aob: aob,
            new_aob: sig.pattern,
            at: image.label(site),
            matched: score,
            of: pattern.fixed_len(),
        });
    }
    if !fixed.is_empty() {
        out.meta.pack_version = bump(&out.meta.pack_version);
        if let Some(sha) = process.exe_sha256()
            && !out.game.builds.iter().any(|b| b.sha256 == sha)
        {
            out.game.builds.push(Build {
                sha256: sha,
                label: Some("repaired by JuiceBox Trainer".into()),
                verified: false,
            });
        }
    }
    Ok(HealReport {
        ok,
        fixed,
        broken,
        pack: out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bumps_versions() {
        assert_eq!(bump("1.0.0"), "1.0.1");
        assert_eq!(bump("0.1.9"), "0.1.10");
        assert_eq!(bump("2"), "2.1");
    }
}
