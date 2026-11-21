---
name: after
description: How long a line may hold before it detaches — read it, or set it for this project.
disable-model-invocation: true
allowed-tools: Bash(jbx after:*)
---

You asked for `jbx after $ARGUMENTS`; it ran when this skill loaded, outside
the agent's shell (where moving the cut is refused), and printed:

!`jbx after $ARGUMENTS`

Show that output as it is. With nothing, it reports the threshold and where
that value came from. With a number of seconds, it wrote it into this
project's `.jbx.yaml`.

`jbx gain --thresholds` is the evidence for choosing one: it replays
what every other cut would have cost on the lines already measured.
