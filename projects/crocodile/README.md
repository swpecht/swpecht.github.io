# Update sim to Warhammer 40k style rules
    [*] Implement unit coherency for movement
    [*] Implement shooting phases
        [*] Implement all phase changes and cycling -- have an end phase action, incl. fix for active team
        [*] Implement legal actions to depends on the phase
        [*] Implement legal actions for shoot
        [*] Implement apply action for shoot, including chance nodes
            * Add field for chance node and for action to resolve
            [*] Implement pending chance action
            [*] Implement resolution for pedning chance action
            [*] Implement test to go through the shoot phase, test going to chance node, etc.
            [*] Implement function to get probabilities for chance action -- see open spiel
        [*] When hovering over weapon action button, highlight the unit that will be hit
        [*] Implement tracking in game state to allow each unit to use each weapon only one time
    [ ] Implement charge phase
        [ ] Make the charge rolls for each unit on the players team
            [ ] Do each unit individually, need a way to track the progress of the chance, each one queues up the next chance action?
            [ ] Or better to just support a list of chance actions that need to be resolved?, then just keep going through all of those
        [ ] Then use the same rules for moving units as for charge, don't need to move every model in a unit all at once
        [ ] Implement UI for charge
    [ ] Implement movement phase
        [ ] models cannot move within engagement range of other models
        [ ] implement Advance mode
        [ ] implement fall back moves -- color code fall back move squares and regular squares? then still a single click where go
    [ ] Implement fight phase
    [ ] Implement Command phase
    [ ] Imlpement victory points, including the per round cards that are drawn or controlling areas. See some 40k in 40min videos
    [ ] Implement passives, like the necron regrowth
[ ] Switch to hexes for world map instead of squares, details: https://www.redblobgames.com/grids/hexagons/
    * Likely want to model everything as a graph for many operations, e.g. path finding we get for free, each node could have a cost to traverse by team, terrain is tough for all, can pass through friends, not enemies
    * Instead of storing coords, could store the index in the grpah, then can lookup by that
[ ] Implement terrain / cover
    [ ] Visiblity for shooting phase
    [ ] Path finding
[ ] Implement more complex shooting
    [ ] Player can choose which model takes damage
    [ ] Different models can target different units
    [ ] Don't allow shooting for things other than pistols in engagement range of another enemy
    [ ] Implement weapon abilities

# QoL:
[ ] Refactor the apply and undo methods for ActionResults to be children of that enum, take in gs as paramter? Or better to just have an opposite function
    * then apply all the opposites?
[ ] Implement highlight for the selected unit
[ ] Add in a log of what actions were taken
[ ] Implement the select unit loop for movement?

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


