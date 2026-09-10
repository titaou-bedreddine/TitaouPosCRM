# Keeping TitaouPosCRM in sync with TitaouPosT

TitaouPosCRM is a fork of **TitaouPosT** with its own direction (cloud sync,
renames, isolation). Fixes and features made on TitaouPosT do **not** flow in
automatically — you decide, commit by commit, which ones apply to both
products. Git's `upstream` remote + `cherry-pick` is the binding.

## One-time setup (already done on this machine)

```bash
cd TitaouPosCRM
git remote add upstream https://github.com/titaou-bedreddine/TitaouPosT.git
git fetch upstream
```

(`git remote -v` shows both: `origin` = TitaouPosCRM, `upstream` = TitaouPosT.)

## Bringing TitaouPosT changes into TitaouPosCRM

```bash
# 1. See what's new since the fork point
git fetch upstream
git merge-base feat/crm-integration upstream/main   # → fork commit
git log --oneline <fork-commit>..upstream/main

# 2. Read a commit before deciding
git show <sha> --stat          # which files
git show <sha>                 # full diff

# 3. Apply ONLY the commits you want (oldest first)
git cherry-pick <sha>

# 4. If a conflict fires (our renames/cloudsync touch the same file):
#    fix the marked file(s), then
git cherry-pick --continue     #   or --skip to drop that commit

# 5. Verify + ship (the usual loop)
cargo test --lib        # in src-tauri
npm run build           # frontend
npm run tauri build     # release installers
```

## Decision guide — does a commit belong in BOTH repos?

| Change touches | Applies to |
|---|---|
| Business bugs (expenses, cash, products, POS flows, LAN) | **Both** — cherry-pick |
| Schema/migrations in `database/mod.rs`, services | **Both** — cherry-pick (our additions live in separate regions; conflicts are rare and shallow) |
| Branding strings (`TitaouPOS` in UI) | Cherry-pick, then re-run the rename sweep: `grep -rn "TitaouPOS" src src-tauri/src` and fix UI strings to `TitaouPosCRM` (comments mentioning the sibling app are fine) |
| Installer/version/updater/GitHub URLs | **Never directly** — TitaouPosCRM has its own repo, feed, ports, app id |
| The walk-in-customer seed, barcode/AZERTY, printing | **Both** — identical code |

After each batch of picks: run the branding sweep + tests, bump the version,
`git push origin feat/crm-integration`, and cut a release.

## The reverse direction

Fixes made in TitaouPosCRM that are product bugs (not cloud-specific) should
be committed on TitaouPosT too — cherry-pick works both ways:

```bash
cd TitaouPosT
git remote add crm https://github.com/titaou-bedreddine/TitaouPosCRM.git   # once
git fetch crm
git cherry-pick <sha>       # pick the fix commit from feat/crm-integration
```

Rule of thumb: if the commit message/diff makes sense without the words
"cloud sync" or "CRM", it belongs in both.

## What was already synced

- **2026-09-09**: `5143148` (offline client login), `f8fa0aa` (terminal_name
  ALTER self-repair), `2c6b063` (11-bug batch: expense edit/delete, product
  APPLY button, AZERTY, true-All stats, local dates, live cross-PC refresh,
  packaging tab, reset dialog), `fa96f6d` (walk-in-customer seed FK bug,
  server self-refresh, packaging/scale tab regression). Plus the rename-sweep
  find: autostart status query now uses the `TitaouPosCRM` registry key.
