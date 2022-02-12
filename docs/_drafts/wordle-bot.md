---
layout: post
title:  "Creating a Wordle bot in rust"
categories: project-log
---

## Perf improvements

Start:

* Flamegraph: flamegraph-start
* bench: [2.1158 s 2.1405 s 2.1681 s]

Findings:

* Most time spent in `increment_count` -- switch to a fixed size array for letters?

Results:

* Flamegraph: flamegraph-increment_count_fix
* bench: [625.23 ms 630.78 ms 636.73 ms], -70%
