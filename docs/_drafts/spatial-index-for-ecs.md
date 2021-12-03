---
layout: post
title:  "Implementing exploration and pathfinding"
categories: project-log
---

## Context

Working on a concept for a game.
[mechanics]

## Problem

## Approach

## Results

## Start

Benchmarking find path 20x20: Warming up for 3.0000 s
Warning: Unable to complete 100 samples in 5.0s. You may wish to increase target time to 7.2s, or reduce sample count to 60.
find path 20x20         time:   [71.737 ms 72.456 ms 73.222 ms]
                        change: [+57.311% +59.159% +61.079%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild

From flamegraph, 56% of the time is in `get_entity`

We re-create the cache each time

Switching to a cache for entity lookups: `features.enable_entity_spatial_cache = true;`


find path 20x20         time:   [47.588 ms 47.837 ms 48.138 ms]
                        change: [-52.704% -51.547% -50.521%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  4 (4.00%) high mild
  3 (3.00%) high severe

No visible time spent in get entity on flamegraph.
