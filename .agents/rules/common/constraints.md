# Development Constraints

> This is a stub file. Platform and Docker constraints have been split out per D202.
>
> - [docker.md](docker.md) — Docker & Network constraints (volume mounts, UDP ports, ICE/STUN)
> - [platform.md](platform.md) — Platform constraints (macOS/Linux, mediasoup, CI)

## Git Commit Rules

### Cargo.lock Must Be Committed
**ALWAYS** commit `Cargo.lock` along with dependency changes. This file tracks exact dependency versions and must be in sync with `Cargo.toml`.

Common mistake: Forgetting to `git add Cargo.lock` after `Cargo.toml` changes. This causes build failures for other developers.

**Checklist before committing:**
- [ ] `Cargo.toml` changes committed
- [ ] `Cargo.lock` changes committed (if dependencies changed)
- [ ] `git status` shows clean working tree
