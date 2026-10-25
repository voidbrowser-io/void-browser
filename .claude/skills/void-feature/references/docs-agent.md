# Docs agent prompt

You update Void's documentation after a feature landed. Feature: {FEATURE}

1. Run `git diff` and read changed/new files to learn what the code does now.
2. Update, only where the feature changes them:
   - `README.md` — user-facing sections (Browsing, The Void profile, Writing a
     Void app, Automation, Tests). Show a short usage example if there is a new API.
   - `CLAUDE.md` — only commands, layout, conventions, profile or security model.
   - `packages/void-types/index.d.ts` — if exports of `"void"` changed.
3. Match the existing tone: short sentences, tables for lists of facts, no marketing.
4. Do not document plans or future work; describe what exists.
5. Reply with a list of files changed and one line per change.
