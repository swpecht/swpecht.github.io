# MVP
[*] models cannot move within engagement range of other models
[ ] Don't allow shooting for things other than pistols in engagement range of another enemy
[ ] Don't allow targeting units for shooting that are in engagement range with allies
[ ] Add in additional units -- could we just do a necors vs space marines?
    [ ] 1x Technomancer: https://wahapedia.ru/wh40k10ed/factions/necrons/Technomancer
    [ ] 1x Overlord: https://wahapedia.ru/wh40k10ed/factions/necrons/Overlord-with-translocation-shroud
    [ ] 1x10 Necron Warriors: https://wahapedia.ru/wh40k10ed/factions/necrons/Necron-Warriors
    [ ] 1x5 Lychguard - Shields: https://wahapedia.ru/wh40k10ed/factions/necrons/Lychguard
    [ ] 1x5 Deathmarks: https://wahapedia.ru/wh40k10ed/factions/necrons/Deathmarks
    [ ] 1x3 Skorpekh Destroyers: https://wahapedia.ru/wh40k10ed/factions/necrons/Skorpekh-Destroyers
    [ ] 1x3 Canoptek Scarabs: https://wahapedia.ru/wh40k10ed/factions/necrons/Canoptek-Scarab-Swarms
    [ ] Terminator Librarian: https://wahapedia.ru/wh40k10ed/factions/space-marines/Librarian-In-Terminator-Armour
    [ ] Terminator Captain - Sword: https://wahapedia.ru/wh40k10ed/factions/space-marines/Captain-In-Terminator-Armour
    [ ] Lieutenant - power fist / plas pistor (fire discipline): https://wahapedia.ru/wh40k10ed/factions/space-marines/Lieutenant
    [ ] 1x5 Infernus squad: https://wahapedia.ru/wh40k10ed/factions/space-marines/Infernus-Squad
    [ ] 1x5 Terminators - assault cannon: https://wahapedia.ru/wh40k10ed/factions/space-marines/Terminator-Squad
    [ ] 1x10 Hellblasters: https://wahapedia.ru/wh40k10ed/factions/space-marines/Hellblaster-Squad
    [ ] 2x5 Jump intercessors - power first + plasma pistol: https://wahapedia.ru/wh40k10ed/factions/space-marines/Assault-Intercessors-With-Jump-Packs
[ ] Add terrain
    [ ] Switch to hex grid
    [ ] Visualization of terrain
    [ ] Pathfinding around terrain
    [ ] Line of sight for shooting
[ ] Add vehicles nad monsters
    [ ] Void Dragon
    [ ] Canoptek Doomstalker
[ ] Implement passives
    [ ] Necron regrowth: Reanimation protocal
    [ ] Oath of moment
    [ ] Re-roll charge rolls for model unit
    [ ] Fire discipline
    [ ] Fury of the first
[ ] Get a baseline playable version of the AI
[ ] Implement army rules

Starter armies: https://www.youtube.com/watch?v=Mg5pQxDobPs&ab_channel=AuspexTactics

# AI:
[ ] Add support for chance nodes
[ ] Add time based deadline -- stop search after X time

# QoL:
[ ] Add autoselect for whoever's turn it is to go
[ ] Refactor UI parts to be in separate files, see left panel ui
[ ] Refactor the apply and undo methods for ActionResults to be children of that enum, take in gs as paramter? Or better to just have an opposite function
    * then apply all the opposites?
[ ] Implement highlight for the selected unit
[ ] Add in a log of what actions were taken
[ ] Implement the select unit loop for movement?
[ ] Switch to hexes for world map instead of squares, details: https://www.redblobgames.com/grids/hexagons/
    * Likely want to model everything as a graph for many operations, e.g. path finding we get for free, each node could have a cost to traverse by team, terrain is tough for all, can pass through friends, not enemies
    * Instead of storing coords, could store the index in the grpah, then can lookup by that
[ ] Implement warning if ending turn shooting actions still available

# UI Cleanup
[ ] Show hit and miss text for each shot - right now only shows one for each model, so broken for guns with multiple shots

# Gaps in Warhammer rules
[ ] Implement Command phase
[ ] Implement terrain / cover
    [ ] Visiblity for shooting phase
    [ ] Path finding
[ ] Implement more complex shooting
    [ ] Player can choose which model takes damage
    [ ] Different models can target different units
    [ ] Implement weapon abilities
[ ] Implement more complex fight phase
    [ ] Implement fight first phase
[ ] Implement more complex movement
    [ ] implement Advance mode
    [ ] implement fall back moves -- color code fall back move squares and regular squares? then still a single click where go
[ ] Imlpement victory points, including the per round cards that are drawn or controlling areas. See some 40k in 40min videos

# Game data
[ ] Create space marine starter army
    * https://www.dakkadakka.com/wiki/en/Starter_Army_Lists_for_each_40k_Race_with_Costs%21#500_Pts._11
[ ] Create necron starter army
    * https://wahapedia.ru/wh40k10ed/factions/necrons/Necron-Warriors


# Game mechanic ideas
[ ] What if the ai stored moves from previous runs to better learn scores for it's moves and improve against players over time?


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


