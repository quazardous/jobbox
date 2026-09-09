---
name: Something is wrong
about: A command behaved in a way you did not expect
labels: bug
---

**What you ran, and what happened.**


**What you expected instead.**


**The three things that explain most reports.** Please paste them —
they are usually the whole answer, and asking for them costs a round
trip each.

```console
$ jbx config          # every setting, and where it came from
$ jbx health          # what is running, mute, or stranded
$ jbx --version
```

**If nothing is being detached**, that is usually deliberate rather than
broken — see TROUBLESHOOTING.md. jbx steps aside for a terminal, and
again when it is already wrapped by an outer jbx.
