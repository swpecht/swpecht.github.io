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

## Entity lookup cache

find path 20x20         time:   [59.692 ms 59.977 ms 60.279 ms]
                        change: [+943.06% +952.35% +961.77%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 3 outliers among 100 measurements (3.00%)

From flamegraph, 56% of the time is in `get_entity`

We re-create the cache each time

Switching to a cache for entity lookups: `features.enable_entity_spatial_cache = true;`

find path 20x20         time:   [33.759 ms 34.235 ms 34.965 ms]
                        change: [+481.58% +490.94% +502.80%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high severe

No visible time spent in get entity on flamegraph.
