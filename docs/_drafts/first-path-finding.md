---
layout: post
title:  "Exploring a 2d space with Rust"
categories: project-log
---

## Context

I've had the half-idea for a base defence game with an intelligent AI attacker. Think [They Are Billions](https://store.steampowered.com/app/644930/They_Are_Billions/) but the zombies will take the long way around to attack the weak side of your base. This will be part of a series of posts exploring the different systems required to make such a game possible.

Creating a fun game isn't the goal -- learning Rust is. The code can be found [here](https://github.com/swpecht/swpecht.github.io/tree/master/projects/running-emu).

## Problem
Given a starting location and a goal location with unknown terrain in between, build an agent capable of exploring the area to find a low cost path to reach the goal.

The main question we'll focus on is building a system to decide where the agent should explore next, e.g.

$$f(map\ state) \rightarrow point$$

Longer-term, the agent should account for things like damage from player turrets, but for now, let's just have some walls and open tiles.

The map is represented as a 2d board of characters:

```text
....@..........
...............
...............
...............
...............
....WWW........
.WWW...........
.WGW...........
...............
```

Where:

* `@`: the agent that moves around
* `.`: open tiles
* `W`: walls that have a cost of 10 to move over
* `G`: the goal

But the agent can only reveal the tiles immediately arround it. So the starting board looks like:

```text
???.@.?????????
????.??????????
???????????????
???????????????
???????????????
???????????????
???????????????
??G????????????
???????????????
```

Each turn, the agent can move one to an adjacent square. The goal is the reach `G` in the lowest number of turns possible.

## Naive approach

The naive approach is standard breadth first search path finding algorithm.

You can see the results here:

{% include game_state_animation.html game_data='naive_approach.txt' %}

This algorithm takes 472 steps to explore the space. In particular, it wasts a lot of time exploring tiles that aren't any closer to the goal.

We can do better by prioritizing exploring spaces closer to the goal.

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