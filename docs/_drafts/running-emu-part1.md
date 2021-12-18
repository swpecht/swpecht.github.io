---
layout: post
title:  "Weighted A*"
date:   2021-11-10 00:00:00 +0000
categories: project-log
---

## Context

Working on a concept for a game.
[mechanics]

## Problem

Implement path finding on a small board

Status:

* Simple pathing with varying cost (done)
* Implement A* for more efficient path finding? Although need to integrate with part of the policy for exploration (done)
  * Compare the number of 'steps' needed (done)
* Create basic UI to show animation of exploration
  * Figure out how to re-print over lines (done, see bfs function)
  * Re-factor to something closer to gameplay loop to see how the agent performs over time, need 'update' ticks (done)
  * Finish refactoring to ECS, e.g. entity for tiles, and eventually controlled character.
    * Implement render system on top of ECS (done)
    * Switch AI to move around a unit '@', e.g. only valid moves are those within range of the unit (done)
    * Refactor ECS to handle dynamic component addition, e.g. <https://ianjk.com/ecs-in-rust/> (done)
    * Add highlighting for planned path of @, implement as component? (done)
* Clean up ECS (done)
  * Add test cases to ECS (done)
  * Clean up iteration patter for systems over ECS. implement a `filter` type call? (abandoned, too complicated)
  * Port to hecs (<https://github.com/Ralith/hecs>) (done)
    * Add some parsing tests (done)
  * Remove `Map` concept, instead do everything on world and normal functions (done)
    * Remove need for width and height (done)
  * Add vision system (done)
  * Add movement system with velocity (done)
  * Fix bug where attacker agent doesn't look at all visible tiles, e.g. vision of 2 doesn't work, re-create the cost matrix each pass? (done)

* Begin optimizing pathfinding (done)
  * Add spatial index for entity lookups (done)
  * Look at LPA (done)
  * Move the logic to handle unrevealed tiles out of pathing algos, give tile costs that reflect things (done)
  * Finish low hanging fruit optimization of loops, look at memory allocation (done)

* Do some performance benchmarks on large maps
  * Add ability to read map from file, create a benchmark suite of maps to run through, test both time and number of steps
  * Create 100x100 map and benchmark (done)

* Concept of sending units that give "life" to explore?
  * Add life to units (done)
  * Add attack to the agent -- update `system_pathing` to attack if there is a W in the way (done)
  * Add a weapon component to store damage? (done)
  * Add tower unit that can do damage in an area, in terms of damage per frame (done)
  * Update cost calculations to be frames to destroy * damage per frame, rather than additive, but need to accound for location, it will now be different costs depending on where the unit is trying to move
    * may need to switch this to be a graph rather than a straight array, could store this as an adjacency matrix (done)
    * Optimize adjacency matrix implementation, use vectors to back it? (done)
    * Switch to arrays and slices? https://doc.rust-lang.org/rust-by-example/primitives/array.html (done)
    * Switch to multi-phase creation -- first one makes all connections, then have an update edgfe function that onjly works on existing edges (done)
    * Switch how unseen tiles are handled, likely want a single version where vision is applied at the end? so can populate without respect for vision, and then remove all edges to invisible tiles -- although creates issues with unseen damage sources -- create annotation for the connection type in nodes, can create views of the graph that filter for certain edge types (done)
    * Add spike trap tile, no health and 0 range attack, can use this as the replacement for walls (done)

* Get rendering working
  * Use SDL? (done)
  * Refactor to move all caches into world object, will make calling code easier

* Clean up some tech debt and fix bugs
  * Refactor how processing pair of entities: <https://www.reddit.com/r/rust_gamedev/comments/fsfqhf/how_to_process_pairs_of_entities_using_ecs/>
  * Switch to a modify health queue so can have multiple damage sources per tick

* Update vision system to be line of sight rather than a set distance, e.g. can't see past walls

* Add support for coordinating multiple agents to explore at once