---
name: void-feature
description: Multi-agent feature workflow for Void Browser — plan, implement with tests, verify in the real browser, then parallel reviewer agents (architecture, correctness/tests, security, Void Profile), fix rounds until clean, a docs agent, and a commit. Use this when the user runs /void-feature, or asks to build, add or implement a feature in Void "properly", "with review", "with agents", "end to end", or wants the implement→review→docs loop, even without naming the skill.
---

# /void-feature

Ship one Void feature through a repeatable loop: build it, prove it works, let
independent reviewers attack it, fix what they find, document it, commit.
The loop exists because a single author misses things that a reviewer with a
narrow focus catches quickly — the tests we added found two real bugs on the
first run.

The user invoking this skill is the go-ahead to use subagents for the review
and docs steps. Tell them up front that the review round runs several agents in
parallel (more tokens than a solo change).

## 0. Understand

- Restate the feature in one or two sentences and list what is explicitly out of scope.
- Load the `void-architecture` skill and run the **Void Profile gate** (section 4).
  If the feature fails the gate, stop and explain why before writing code.
- Ask the user only if a decision is genuinely theirs (e.g. syntax of a new API).

## 1. Plan

Write a short plan into the task list:
- which crates change (respecting the layering diagram),
- the public API / CSS syntax / `--auto` scenario,
- which tests prove it (section 6 of `void-architecture`).

## 2. Implement

- Tests first where the behaviour is crisp (CSS values, layout boxes, engine behaviour).
- Library APIs move fast: read their source in `~/.cargo/registry/src/*/` rather than guessing.
- Keep commits out of this step; work in the tree.

## 3. Verify yourself

Run, and fix until green:

```bash
cargo test --workspace            # use CARGO_TARGET_DIR=target/e2e if a void.exe window is open
npx -y pnpm@10.18.0 -r typecheck
npx -y pnpm@10.18.0 -r test
```

For anything visible, run an `--auto` script ending in `shot <file>.png` and look
at the image. Add the scenario to `apps/browser/tests/e2e.rs`.

## 4. Review round (parallel agents)

Get the diff (`git diff` plus new files) and spawn reviewers **in one message so
they run in parallel**. Use the prompts in `references/reviewers.md`. Pick:

| Reviewer | When |
|---|---|
| Architecture | always |
| Correctness & tests | always |
| Void Profile | when web-facing behaviour changed (CSS, DOM/JS API, HTML handling) |
| Security | when the loader, URLs, `HostCmd`, privileges, networking or files changed |

Each reviewer returns findings as `severity | file:line | problem | fix`, with
severity `blocker`, `should-fix` or `nit`.

## 5. Fix loop

- Fix every `blocker` and `should-fix`. For a finding you disagree with, write one
  line saying why instead of changing code — reviewers can be wrong.
- Re-run step 3.
- Run a second review round only with the reviewers that reported blockers, giving
  them the new diff and their previous findings.
- Stop after round 3 even if nits remain; list leftovers for the user. More rounds
  rarely pay for themselves.

## 6. Docs agent

Spawn one agent with `references/docs-agent.md` to update README.md, CLAUDE.md
(only if commands, layout or the profile changed) and `packages/void-types` (if a
`void` API changed). Review its edit yourself — docs must describe what the code
does now, not what was planned.

## 7. Commit and report

- One commit: imperative subject; body says what and why, lists fixed review
  findings worth knowing; ends with the Co-Authored-By line.
- Do not push unless the user asks.
- Report to the user: what was built, test counts, review rounds and notable
  findings, anything left open, the screenshot path if there was a visual change.
