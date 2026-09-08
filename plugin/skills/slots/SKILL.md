---
name: slots
description: How many queued jobs may run at once, here — read it, or set it for this project.
disable-model-invocation: true
---

Run `jbx slots $ARGUMENTS` and show what it prints.

With nothing, it reports the cap and how much of it is in use. With a
number, or `none`, it writes that into this project's `.jbx.yaml` and
says which file it wrote.
