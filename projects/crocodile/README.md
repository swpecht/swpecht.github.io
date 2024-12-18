[*] Create rough ui for interacting with state
[*] Draw the basic player character
[*] Draw basic tiles
[*] Fix the offset issue with the world cords
[*] Highlight grid square the mouse is on with gizmo outlining it
[*] System to translate a click into simulation coords -- this can set selection
[*] Implement selecting player character with mouse click
[*] Show character based on position in world sim
    [*] Make system to sync display to sim
[ ] Highlight selected character
[*] Implement poc for moving character
[*] Implement right click to move the character around
[*] Implement state for processing action and taking input
[*] Implement attacks
    [*] Need to implement states, then can run the button creation when go into input state
    * How to handle? Have each attack take a list of the targets? Then can figure out the legal list from a separate attack action type stored within the sim entity?
    * Select attack with button
    * Valid squares to select are highlighted based on legal actions
    * https://bevyengine.org/examples/Games/game-menu/ -- for how to make buttons work
    * Likely have an id tied to each button, this is associates it with the vector of legal actions
    [*] Fix bug where buttons don't update on turn end -- force PlayState change on turn end action?
    [*] Have the buttons work with each action
[*] Add a gamestate sync after each processing step is done
    * Allow syncing without creating if it already exists
[*] Add animation for attack
    [*] Melee attack
    [*] Bow attack
[*] Refactor the action handling system to work off events?
    * One system that does dispatch to all the other systems as events?
    * Maybe a projectile event? And associated system
[*] Implement legal actions
[*] Build basic AI for enemy entities
    [*] build is_terminal check
    [*] build a score function
[*] Build minimax AI
    [*] fix where orcs not attacking
    [*] fix the weird movement looping -- add movement actions at the end of legal actions?
    [*] Add profiling test to track overall performance
    [*] Switch to single move rather than chained moves?
    [*] Implement iterative deepening
    [*] Switch to integer score rather than f32?
    [*] Switch depth to be based on number of entity look ahead?
    [ ] Reduce size of the simstate object -- can we factor out some of the details? e.g. pre-define the abilities in a separate list, then just keep a list of integers for what each entity can do?
    [*] Look at other search algorithms
[*] Add in outline for where can move
[*] Change actions to only show when selecting a character
[*] Change to not reset transposition table one each iterative deepedning interation, instead have it check the reamiing depth to search so only take tt result if stronger than normal search
[*] Fix logic error with new slab code, likely related to get_many_mut
[*] Have the game dynamically look up art by character id -- need to create a global store of the info for bevy
[ ] Can we do all this loading with code-gen?

[ ] Create enums that map all the info we need -- more peformant than the lookups?
    * Is this just creating a macro?
[ ] Add stats for how many nodes searched
[ ] Dynamically populate actions from file
[ ] Look at wrapping all the unchaning simstate data into a refecne (Arc? Or just a reference?)


https://news.ycombinator.com/item?id=21037125


[*] Add health bars
[*] Implement the undo function with action results
[*] Add test for undo function
[*] Fix idle animation
[*] Change the interpolation between gamestates to use the actionresult rather than actions themselves -- can just apply them all at once, make a meleee attack, charge, etc.

[*] Switch to using json files to pull sprite information
    [ ] Add sword to idle animation
    [*] Add running animation
[ ] Implement attack of opportunity
    [ ] Fix bug with undo -- found in test, doing something wrong with action point tracking -- seems like getting double spent
    * Seems like the issue is related to ending up with an enemy where the current character ends up with 0 health, is the damage incorrect?
    * Or maybe the wrong target?
[ ] Implement Giant Goats
    [ ] Create new art for the goats
    [*] Implement the displacement for charge
    [ ] Implement the movement cost for charge
    [ ] Implement attack of opportunity
    [ ] Implement Knockdown effect and prone results
    [ ] Implement action and effect log in the UI
    [ ] Implement actual randomness for running the sim for the game itself rather than search, have an apply action expectation (current one), and an apply action that takes an RNG?
    [ ] Implement visual effect for going prone

[ ] Implement additional play character and enemies


Maybe:
[ ] Refactor sim state to enable undo and redo


## thoughts on implementing undo
The idea of creating the effects of every action, e.g. dmg X, spend action on unit Y, move Z is becoming more appealing.

We could keep this a list of all actions -- including a generation when they were applied. Undoing a turn would be as simple as popping everything in that generation. This would also give us a way to query all the action results from the previous turn, making it easier to figure out what to translate the gamestate transition into visuals.

Rough design:

Have ActionResult enum. Implement a way to apply and unapply every possible action result. Trick things:
* Status effects: should be a need to apply them, but could do it as a diff if needed, e.g. adding turns to something.
* How do we put an enemy that dies back into the proper place? -- can have a RemoveEntity action that we can dynamically create if we notice an enemy has died
* How do we handle resetting stats at the start of a turn? -- can we just ignore things like SpendActionPoint and do a reset at the end of undo? But how we we know this was a generation when a turn ended? Special case? -- if we do the reset through a series of action events (as diffs between where we are and reset state) -- this gives us an easy way to get back to the pre-end turn state

How it works:
* Convert each action into ActionResult list
* process each action result, applying it's changes to all items
* create an undo for each action result


Benchmark results
* Baseline: 63,289,459.20 ns/iter (+/- 11,958,311.88)
* Multi-step move: 164,872,726.00 ns/iter (+/- 93,314,993.13)
* 2 character look ahead: 221,082,558.80 ns/iter (+/- 2,627,776.55) 
* Sorted moves: 493,477,100.10 ns/iter (+/- 88,491,696.70)
* Iterative deepening: 187,976.70 ns/iter (+/- 8,288.82)
* 4 steps: 537,902,490.80 ns/iter
* Fix bug where movement only reduced by 1: 39,336,430.60 ns/iter
* 5 moves ahead: 255,294,112.20 ns/iter (+/- 71,430,435.09)
* Remove allocation for movement legal actions: 241,639,675.50 ns/iter (+/- 86,191,212.92)
* Remove allocation for ability legal actions: 405,635,100.60 ns/iter (+/- 202,901,593.74)  
* Add in system to only iterate over world coords in range: 123,592,024.30 ns/iter (+/- 9,580,663.70)
* 5 moves w/o move ordering: 
* 5 moves w/ increasing depth: 424,017,204.00 ns/iter (+/- 212,148,153.94)
* Search PV first: 253,837,193.00 ns/iter (+/- 31,015,222.91)
* Add in checks for depth: 429,567,544.60 ns/iter (+/- 96,946,865.88)
* Don't reset transposition table between moves: 433,278,822.00 ns/iter (+/- 6,675,863.64)
* Single action vec: 441,645,617.10 ns/iter (+/- 121,344,308.84)
* Change to incrementing depth each call and switch to 8: 614,031,799.10 ns/iter (+/- 29,827,364.16)
* Slab: 88,404,085.30 ns/iter (+/- 24,453,605.13) 
* Switch to 5 moves: 165,896,103.70 ns/iter (+/- 101,483,562.71)
* Re-use array to get child moves: 139,839,098.80 ns/iter (+/- 13,923,816.76) 
* Remove deserialize call when creating new simstates for slab: 74,004,622.10 ns/iter (+/- 2,156,366.59)
* Most recent: 15,477,548.50 ns/iter (+/- 12,629,083.19)
* 5 moves with the new action result queue: 17,332,275.50 ns/iter (+/- 11,693,912.79)
* 5 moves with undo:  11,299,049.00 ns/iter (+/- 633,671.14)
* Switch to 6 moves:  194,917,867.30 ns/iter (+/- 38,836,177.15)
* Confirm starting with PV move