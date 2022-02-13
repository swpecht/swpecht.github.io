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

Still too slow. Dominated by filter answers

Rather than evaluating as if each answer is true, do we evaluate on all possible scores? Only 3^5 possible options, can then weight the liklihood by the number of responses.

Would reduce the number of filter answers calls from 2k to ~200 (3^5)

Before:

* Bench:  [1.0161 s 1.0229 s 1.0315 s]

After:

* Bench: [124.59 ms 126.04 ms 127.87 ms], -87%

Significant reduction
