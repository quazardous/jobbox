---
name: after
description: How long a line may hold before it detaches — read it, or set it for this project.
disable-model-invocation: true
---

Run `jbx after $ARGUMENTS` and show what it prints.

With nothing, it reports the threshold and where that value came from.
With a number of seconds, it writes it into this project's `.jbx.yaml`.

`jbx gain --thresholds` is the evidence for choosing one: it replays
what every other cut would have cost on the lines already measured.
