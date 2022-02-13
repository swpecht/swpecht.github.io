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

Significant reduction, but still about too slow to evaluate all guesses in realtime. Most time is now spent in hashset related work, specifically on removing items.

For the final iteration, only iterate over remaining items,

* Bench:  [64.390 ms 64.798 ms 65.236 ms], -46%

Could you create a multi-level hash function:

* Mask based on letter counts, 26 long array of counts, would need to support maybe
* Mask based on position for final filtering
HashMap<letter_count>

`cargo run --release` significantly speeds up execution

Still too slow for evaluation,

## Switch to vector for filtering

* Realize that usually filter most values, so spending a lot of time doing hashset removes, what is create a separate vec and move items over as needed

Bench:  [13.343 ms 13.407 ms 13.485 ms], -75%
Flamegraph: vector-filter

## Pre-size vectors

Minimal change

## Improved char indexing

Store words in a char array rather than string, allows for faster indexing
Bench: [5.8062 ms 5.9414 ms 6.1319 ms], -56%
flamegraph: array-words

## Exit char count checking early

Bench: [4.4250 ms 4.4978 ms 4.5848 ms], -24%

## Next

* Some way to filter based on char counts?
* Do a bloom filter like hash function, 5*26 bit mask to represent the words, or just start with a 26 mask for which letters are contained
* Look 2-3 steps ahead to evaluate opening guess, or just on expected score
