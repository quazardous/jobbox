# Coming from `jobbox`

`jobbox` became `jbx` in 0.5.0, on 05/09/2026, and there is no alias — a
second name kept for old callers is read later as intended, and nobody
removes it. The `jobbox` skill went in the same release; the plugin's
`/jbx:jbx` is its replacement.

Most verbs kept their name. **One did not, and renaming it by hand goes
wrong without a word:**

| `jobbox` (0.4 and before) | `jbx` |
|---|---|
| `jobbox run <intent> -- <cmd>` | **`jbx queue <intent> -- <cmd>`** |
| `jobbox timings` | `jbx gain` |
| `jobbox observe` | gone — `jbx hook` answers the harness itself |
| `jobbox config`, `health`, `init`, `signals` | the same verb under `jbx` |

**`jobbox run` is `jbx queue`, not `jbx run`.** The old `run` handed work
over with a name and returned at once. Today's `jbx run -- <line>` runs
the line in front of you and lets go only if it outlasts the cut — and a
word before `--` is not read as an intent. `jbx run build -- make` runs
`make` in front of you: no queue, no cap, no name, and `build` dropped
without a word. The command still works, which is why the rename looks
right. Found on 16/09/2026, in a Makefile still calling `jobbox run`.
