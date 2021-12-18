---
layout: post
title:  "Creating an AI to esplore 2d space with Rust"
categories: project-log
---

## Context

I've had the half-idea for a base defence game with an intelligent AI attacker. Think [They Are Billions](https://store.steampowered.com/app/644930/They_Are_Billions/) but the zombies will take the long way around to attack the weak side of your base. This will be part of a series of posts exploring the different systems required to make such a game possible.

Creating a fun game isn't the goal -- learning Rust is. The code can be found [here](https://github.com/swpecht/swpecht.github.io/tree/master/projects/running-emu).

As a caveat, this post is about AI in the game sense. Not in the pile of linear algebra sense.

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

At a high level, we can move to any visible square. We'll evaluate all visible squares. And then choose the one with the best score to move to.

The naive approach is similar to a breadth first search path finding algorithm. We'll evaluate each square based on it's distance from our original starting point. This will give us a good path for future agents to follow.

First we need to know the cost of traveling each of the visible tiles. I'm using [hecs](https://github.com/Ralith/hecs) as the ECS to store world state.

```rust
/// Return the cost matrix from currently visible tiles
/// 
/// This function essentially iterates through all tiles, creating a matrix
/// with 0 for `.` tiles and 10 for `W` tiles
fn get_tile_costs(world: &World) -> Vec<Vec<Option<i32>>> {
    let max_p = get_max_point(world);
    let mut tile_costs = vec![vec![None; max_p.x]; max_p.y];
    for (_, (pos, visible, spr)) in world
        .query::<(&Position, &Visibility, &Sprite)>()
        .into_iter()
    {
        if visible.0 {
            tile_costs[pos.0.y][pos.0.x] = Some(get_tile_cost(spr.0));
        }
    }

    return tile_costs;
}
```

Then we need to know the cost to travel to each of the tiles from the starting point. This is the BFS algorithm for path finding.

```rust
/// Return the lowest travel cost matrix for all visible tiles if possible
///
/// None means no path is possible or there isn't tile information
fn get_travel_costs(start: Point, tile_costs: &Vec<Vec<Option<i32>>>) -> Vec<Vec<Option<i32>>> {
    let width = tile_costs[0].len();
    let height = tile_costs.len();
    let mut travel_costs = vec![vec![None; width]; height];

    let mut queue: PriorityQueue<Point, Reverse<i32>> = PriorityQueue::new();
    queue.push(start, Reverse(0));
    travel_costs[start.y][start.x] = Some(0);

    while !queue.is_empty() {
        let (node, _) = queue.pop().unwrap();
        let distance = travel_costs[node.y][node.x].unwrap();

        let neighbors = get_neighbors(node, width, height);

        for n in neighbors {
            let d = travel_costs[n.y][n.x];

            // Only path over areas where we have cost data
            if d.is_none() && tile_costs[n.y][n.x].is_some() {
                let new_cost = distance + 1 + tile_costs[n.y][n.x].unwrap(); // Cost always increases by minimum of 1
                travel_costs[n.y][n.x] = Some(new_cost);
                queue.push(n, Reverse(new_cost));
            }
        }
    }

    return travel_costs;
}
```

Given both of these, we can start constructing a matrix for the score of each possible tile we can navigate to. We then choose the tile with the lowest score as the next one to explore.

```rust
for y in 0..max_p.y {
    for x in 0..max_p.x {
        let p = Point { x: x, y: y };
        if let Some(cost) = travel_costs[p.y][p.x] {
            candidate_matrix[x + y * max_p.x] = Some(cost)
        }
        let neighbors = get_neighbors(p, max_p.x, max_p.y);
        let mut all_neighbors_visible = true;
        for n in neighbors {
            let e = get_entity(world, n).unwrap();
            let vis = world.query_one::<&Visibility>(e).unwrap().get().unwrap().0;
            all_neighbors_visible = all_neighbors_visible && vis;
        }

        // No reason to visit if all visible and not the goal
        if all_neighbors_visible && p != goal {
            candidate_matrix[x + y * max_p.x] = None;
        }
    }
}

let min_val = candidate_matrix
    .iter()
    .filter_map(|c| c.as_ref())
    .min()
    .unwrap();

let mut min_p = Point { x: 0, y: 0 };
for y in 0..max_p.y {
    for x in 0..max_p.x {
        if candidate_matrix[x + y * max_p.x] == Some(*min_val) {
            min_p = Point { x: x, y: y };
            break;
        }
    }
}
```

You can see the results here:

{% include game_state_animation.html game_data='naive_approach.txt' fig_name='naive' %}

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

[NEED to figure out how to support multiple gifs at once, likely just want to make gifs rather than do anything more complicated.]

## Reduce calls to get_path -- use a calculated travel matrix

From: find path 20x20         time:   [33.759 ms 34.235 ms 34.965 ms]
                        change: [+481.58% +490.94% +502.80%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high severe


To: find path 20x20         time:   [5.6577 ms 5.6994 ms 5.7422 ms]
                        change: [-83.714% -83.352% -83.073%] (p = 0.00 < 0.05)
                        Performance has improved.


## Add A* to djikstra's algorithms

for the travel matrix calc, use a minimum distance across all endpoints as the A* heuristic.

```rust
let new_cost = distance + 1 + tile_costs[n.y][n.x].unwrap(); // Cost always increases by minimum of 1
travel_costs[n.y][n.x] = Some(new_cost);
let heuristic = end_points.iter().map(|x| n.dist(x)).min().unwrap_or(0);
queue.push(n, Reverse(new_cost + heuristic));
```

find path 100x100       time:   [1.4218 s 1.4443 s 1.4678 s]
                        change: [+178.37% +185.62% +192.62%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild

## Other pathfidning algorithms, LPA*

https://en.wikipedia.org/wiki/Lifelong_Planning_A*

With A*:
find path 100x100       time:   [508.55 ms 513.37 ms 518.59 ms]
                        change: [-1.5160% +0.2276% +1.9708%] (p = 0.80 > 0.05)
                        No change in performance detected.

With LPA*:
find path 100x100       time:   [147.91 ms 149.18 ms 150.48 ms]
                        change: [-73.413% -72.822% -72.289%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild


## Look at alternative heaps, e.g. Fibinacci heap

https://en.wikipedia.org/wiki/Fibonacci_heap