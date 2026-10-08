# Wizard

Generate an interactive bash wizard that walks a human through steps only they can perform: provisioning infrastructure, setting up credentials or CI secrets, walking an unfamiliar third-party dashboard, or running a one-off migration or cutover.

## Process

### 1. Scope the procedure
Work out every manual step the human must take and every value that gets captured. Read the repo first:
- For setup: `.env`, `.env.example`, `README`, `docker-compose*`, framework config, `.github/workflows/*`
- For migration: current state, target state, irreversible actions between

Show the user the ordered list of stages and confirm.

### 2. Map each stage's journey
Write the precise path: which URL to open, what to do there, where a value is shown, which variable it fills.

### 3. Author the wizard
Copy `template.sh` to the target path. Replace the example stage with one `stage()` per step. Use library helpers: `stage`, `say`/`step`, `open_url`, `ask`/`ask_secret`, `write_env`, `set_secret`/`set_var`, `pause`/`confirm`. Set `TOTAL_STAGES` to match.

### 4. Verify and hand off
- `bash -n <script>`; run `shellcheck` if available
- `chmod +x <script>`
- Don't run it end-to-end yourself (it blocks on human input)
- Tell the user how to run it

## Wizard library helpers

- `stage "Name"` — clears screen, announces stage with progress
- `say "..."` — plain instruction line
- `step "..."` — numbered action for the human
- `open_url URL` — opens in browser (cross-platform)
- `pause "msg"` — waits for human confirmation
- `confirm "question"` — y/N gate
- `ask KEY "Prompt"` — reads visible value into $KEY
- `ask_secret KEY "Prompt"` — reads hidden value into $KEY
- `write_env KEY VALUE` — upserts into .env
- `set_secret NAME VALUE` — sets GitHub Actions secret
- `finish` — shows closing summary

## Rules

- Never hand-edit the library above the STAGES marker
- Open the URL before asking for its value
- Use `ask_secret` for anything secret
- `confirm` before any irreversible action
- Each `stage` clears the screen — keep one focused task per stage
