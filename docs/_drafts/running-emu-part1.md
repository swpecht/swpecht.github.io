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

* Concept of sending units that give "life" to explore?
  * Add life to units
  * Add tower unit that can do damage in an area, in terms of damage per frame
  * Update cost calculations to be frames to destroy * damage per frame
  * Then could add in destructable walls? For future units to make it easier?

* Do some performance benchmarks on large maps
  * Fix bug where not properly accounting for cost when doing pathing to intermediate goal (done)
  * Fix issue with backtracking while exploring the map, add heuristic to explore spaces close to the agent (done)
    * Fix bug where S and the first position for @ aren't the same, only specify the @ in maps and then have start be shown from there (done)
  * Finish back tracking exploration, see below (done)
    * Fix bug to update cost of tiles when re-visited (done)
  * Add ability to read map from file, create a benchmark suite of maps to run through, test both time and number of steps
  * Create 100x100 map and benchmark

* Add support for coordinating multiple agents to explore at once


