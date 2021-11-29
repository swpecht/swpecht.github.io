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
* Clean up ECS
  * Add test cases to ECS (done)
  * Clean up iteration patter for systems over ECS. implement a `filter` type call? (abandoned, too complicated)
  * Port to hecs (<https://github.com/Ralith/hecs>) (done)
    * Add some parsing tests (done)
  * Remove `Map` concept, instead do everything on world and normal functions (done)
    * Remove need for width and height (done)
  * Add vision system
  * Add movement system with velocity

* Do some performance benchmarks on large maps
  * Fix bug where not properly accounting for cost when doing pathing to intermediate goal (done)
  * Fix issue with backtracking while exploring the map, add heuristic to explore spaces close to the agent (done)
    * Fix bug where S and the first position for @ aren't the same, only specify the @ in maps and then have start be shown from there (done)
  * Finish back tracking exploration, see below (done)
    * Fix bug to update cost of tiles when re-visited (done)
  * Add ability to read map from file, create a benchmark suite of maps to run through, test both time and number of steps
  * Create 100x100 map and benchmark

* Concept of sending units that give "life" to explore? And "steps" of the exploration.
  * Then could add in destructable walls? For future units to make it easier?
* Improve ECS and add querying, possibly switch to an Archetype ECS
  * <https://github.com/SanderMertens/flecs/blob/master/docs/Quickstart.md>

## A* impact

Commit: 2b70fab159dcebe01e691670e41ad36eed98716a

For map:
....S..........
............WWW
...............
............WWW
...............
....WWW........
.WWW.......WWW.
.WGW.......W.W.
...............

A* takes us from 106 steps to 50 steps.

## New hesuristics to account for current location fo agent

.....@.........
............WWW
...............
............WWW
...............
....WWW........
.WWW.......WWW.
.WGW.......W.W.
...............

Takes 338, commit: 4b333735f65e8b9dc903651422f307ec3dd7bab7

Using the exploration vector that accounts for distance from goal: 145 step, commit: 2e21c11
Adding consideration for distance from the current agent: 19 steps, commit: 5677d27

Starting to see some examples where the agent will go backwards, e.g. one candidate could be 1 step closer to the goal, but the other could be 1 step cheaper to get to from the start. Is the naive "add the costs" approach, both of these candidates are considered equal.

For example, given the following state:
????S..????????
????...????????
????...????????
???....????????
??2....????????
?1@.WW?????????
??WW???????????
??G????????????
???????????????

It would seem intuitive to move to 1 next, but the system is recommending moving to 2.

Score(point) = cost_from_start + goal_distance (taxi cab) + agent_distance
Score(2) = 7 + 3 + 1 = 11
Score(1) = 9 + 3 + 1 = 13

Point 2 has the lower score. But it's clear that you'd need to go through a wall to get there. This isn't ever going to be the optimal move.

Need to use something more complicated for a goal distance function that takes into account known squares and treats unknown squares as a set value.

We update the goal_distance function to use the actual cost of navigating squares with an assumption of no incremental cost for navigating unexplored squares.

Can explore this in 17 steps, commit: 06882d5f60f3f2fcfe00c19e7881d55ad673fccb

????.S.????????
????...????????
????...????????
???....????????
.......????????
....WW?????????
.WWW???????????
.WGW???????????
....???????????
