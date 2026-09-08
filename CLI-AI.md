# The agent CLIs jbx answers

jbx is a hook. Something runs a shell command on a model's behalf, jbx
sees the line first, and wraps it so a long one can detach itself.

Five programs offer that hook. **They agree on almost nothing** — not the
name of the shell tool, not the name of the event, not the shape of the
answer, not even the file the hook is declared in. So each has its own
row in a table, and its own section here.

```console
$ jbx hook --list
claude   Bash                 PreToolUse   ~/.claude/settings.json
gemini   run_shell_command    BeforeTool   ~/.gemini/settings.json
droid    Execute              PreToolUse   declare by hand
cursor   Shell                preToolUse   declare by hand  (no unasked endings)
copilot  bash                 preToolUse   declare by hand
```

**`jbx init` declares for the first two.** The other three keep their
hooks in a differently named file with a different structure, so `init`
says so and writes nothing rather than installing a shape that would be
ignored. Their sections below give the exact JSON.

Every shape here was read in that client's own reference and checked
against a probe. That matters more than it sounds: a widely used proxy
watches for `Bash` on Cursor, Droid and Copilot, where their references
say `Shell`, `Execute` and `bash` — three hooks that would never fire
while looking perfectly installed.

## One rule before any of them

**Write the absolute path to the binary.** A hook runs with whatever
`PATH` the client happens to have, which is not the one you installed
with. A bare `jbx` in a hook is `command not found` in place of every
command on the machine — the first test ever written for this found
exactly that.

Every example below uses `/home/you/.local/bin/jbx`. Substitute yours:

```console
$ command -v jbx
```

**And name the client.** `jbx hook` with no name answers as Claude, which
is right for Claude and silently wrong everywhere else: the hook fires,
finds an event name it does not recognise, and returns quietly. Installed,
matching, useless.

---

## Claude Code

```console
$ jbx init
```

That is the whole thing. It also displaces rtk's hook if you have one and
calls rtk itself, because two hooks rewriting the same field is a race no
harness documents — `jbx init --undo` puts it back exactly.

Declared in `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          { "type": "command", "command": "/home/you/.local/bin/jbx hook claude" }
        ]
      }
    ]
  }
}
```

**Or install the plugin**, which carries the binary, the hooks and a
background watch in one piece. The plugin is the *announcing* install: it
declares `Stop`, `UserPromptSubmit` and `SessionStart` as well, where
`jbx init` alone declares only the hook that wraps. See
[README.md](README.md).

## Gemini CLI

```console
$ jbx init --cli gemini
```

Declared in `~/.gemini/settings.json`, in the same shape Claude uses:

```json
{
  "hooks": {
    "BeforeTool": [
      {
        "matcher": "run_shell_command",
        "hooks": [
          { "type": "command", "command": "/home/you/.local/bin/jbx hook gemini" }
        ]
      }
    ]
  }
}
```

Two differences worth knowing, both handled for you:

- Gemini **merges** what a hook returns into the model's arguments, where
  Claude **replaces** them outright. Under Claude a field left out is a
  field deleted, `timeout` included.
- Holding a session open is `deny` here and `block` there. Same effect,
  different spelling, and the wrong one is an unknown value rather than
  an error anybody sees.

`jbx init --cli gemini --announce` adds `AfterAgent`, `BeforeAgent` and
`SessionStart` — Gemini's own names for the three events Claude calls
`Stop`, `UserPromptSubmit` and `SessionStart`.

## Factory Droid

`jbx init` cannot declare this one: Droid keeps its hooks in
`~/.factory/hooks.json` with the events at the **root**, not under a
`hooks` key.

```json
{
  "PreToolUse": [
    {
      "matcher": "Execute",
      "hooks": [
        { "type": "command", "command": "/home/you/.local/bin/jbx hook droid" }
      ]
    }
  ]
}
```

Its shell tool is called **`Execute`**. Everything else matches Claude —
same envelope, same event names — so `Stop`, `UserPromptSubmit` and
`SessionStart` are available if you want endings reported unasked.

A project-level `.factory/hooks.json` works the same way.

## Cursor

`~/.cursor/hooks.json`, and the shape is its own: a `version`, and a flat
`command` string with no `type`.

```json
{
  "version": 1,
  "hooks": {
    "preToolUse": [
      {
        "matcher": "Shell",
        "command": "/home/you/.local/bin/jbx hook cursor"
      }
    ]
  }
}
```

**`preToolUse`, not `beforeShellExecution`.** Cursor has both, and the
other one cannot help: its reference says the output "does not support
modifying the command itself — only controlling whether execution
proceeds". Choosing the right event was half the work.

Its shell tool is **`Shell`**. A project-level `.cursor/hooks.json` works
the same way.

Cursor's turn-level events have not been read, so jbx declares none of
them: `--announce` has nothing to offer here yet, and `jbx wait <id>`
carries endings in the meantime.

## GitHub Copilot CLI

`~/.copilot/hooks/*.json` — a directory of files, and the shape is
furthest from the rest: `bash` and `powershell` keys instead of
`command`, and no nesting under a matcher.

```json
{
  "version": 1,
  "hooks": {
    "preToolUse": [
      {
        "type": "command",
        "bash": "/home/you/.local/bin/jbx hook copilot",
        "powershell": "C:\\Users\\you\\.local\\bin\\jbx.exe hook copilot"
      }
    ]
  }
}
```

Two things to know:

- Its shell tool is **`bash`**, lowercase, and it sends `toolName` and
  `toolArgs` where every other client says `tool_name` and `tool_input`.
  jbx reads both spellings; you do not have to care, but a hook written
  by hand for another client will find nothing here.
- **It needs a recent Copilot CLI.** Rewriting was ignored outright until
  [github/copilot-cli#2013](https://github.com/github/copilot-cli/issues/2013)
  was fixed, and an older version runs the original line rather than
  failing — which is the quiet way for this to be wrong. If commands are
  not being wrapped, check your version first.

Copilot's `agentStop`, `userPromptSubmitted` and `sessionStart` are the
equivalents of Claude's announcing three, if you want them.

---

## Two that are deliberately absent

They are recorded so nobody investigates them again from nothing.

**Mistral Vibe** — hooks shipped experimental in v2.15.0 and changed
again by v2.21.0, declared in `hooks.toml`. The shape is moving and its
reference has not been read in full.

**Meta Muse Code** — no source states whether its `PreToolUse` can
**rewrite** at all, as opposed to allow, deny and ask. Its hooks live
behind an experimental plugin flag, its documented `.muse/hooks.json` is
reported to be silently ignored by the binary, and — the part that
decides it — unknown output keys are said to *fail* the hook, so guessing
would break it rather than be ignored. One question settles this: can it
rewrite?

## Adding one

A row goes in `src/dialect.rs` when its client's **own reference** has
been read and agrees with a probed shape. Not documentation alone, and
not another tool's belief about it.

`jbx describe` publishes the table as `x-jbx-dialects`, and a test holds
every shape still — so changing one without re-measuring fails a build
rather than somebody's session. That is deliberate friction: a hook
speaking the wrong dialect answers nothing and looks perfectly healthy,
which makes a wrong row worse than a missing one.
