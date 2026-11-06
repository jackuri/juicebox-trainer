# Sending this to Claude Code

## 1. One-time setup on your PC (PowerShell)
```powershell
winget install Rustlang.Rustup
winget install OpenJS.NodeJS.LTS
winget install astral-sh.uv
winget install Microsoft.VisualStudio.2022.BuildTools   # tick "Desktop development with C++"
# WebView2 is already on Windows 10/11
rustup default stable
npx rea-agents setup            # choose Claude Code, approve
```
Inside Claude Code:
```
/plugin marketplace add rehan-remade/universal-modder
/plugin install universal-modder@universal-modder
```

## 2. Create the repo
```powershell
mkdir C:\dev\opentrainer; cd C:\dev\opentrainer
# copy CLAUDE.md, KICKOFF.md and the docs\ folder in here
git init
claude
```

## 3. Paste this as your first message
```
Read CLAUDE.md and docs/PLAN.md fully. Then:
1. Summarize the plan back to me in 10 bullets and list anything you think is wrong, risky or missing.
2. Propose the exact folder layout and the crates/packages you'll use for v0.1.0–v0.2.0.
3. Wait for my OK, then build v0.1.0 (skeleton + CI + testbed/native game) and v0.2.0 (process attach,
   typed read/write, module map, anti-cheat guard with tests).
4. Run all checks, update STATUS.md and CHANGELOG.md, commit, and tell me how to run the app and the
   testbed so I can try attaching to it.
Stop after v0.2.0.
```

## 4. Each later session
```
Read STATUS.md and docs/PLAN.md. Continue with the next roadmap version. Same rules: checks pass,
bump version, update STATUS.md + CHANGELOG.md, commit, then stop and show me how to test it.
```
