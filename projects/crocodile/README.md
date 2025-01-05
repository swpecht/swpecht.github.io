# Update sim to Warhammer 40k style rules
    [ ] Implement unit coherency for movement
        * Base entity for the gamestate should be a model -- some models (like Librarian, can be attached to units)
        [*] Allow all entities on a team to move and act at once, makes keeping track or turn ordering easier
        [*] Fix highlight to use z-layer rather than gizmos
        [*] Only show movement highlight for the selected model
        [*] Implement moving the first model in a unit
        [*] Add highlight for unit coherence
        [*] Implement movement constraints on unit -- don't constrain the legal moves, but constrain the moves models in that unit can take?
            * Or don't worry about any of this -- but just desrtoy the units that aren't in unit coherence at the end of the turn
        [*] Add game logic for removing incoherent units
        [*] Add ui for removing units for lack of coherency at end of movement phase
            [*] Fix bug where occupied is checking for a destroyed model, counting as hit
            [*] Fix bug where sprite still showing up after the remove model action
        [ ] Fix bug where two units with a single gap then another unit results in unit coherency
            [ ] May want to refactor out the sim logic to a different crate for easier debugging
        [ ] Fix bug where removing unit doesn't fix coherency issue -- need to add a test for this
    [ ] Implement shooting phases
    [ ] Implement charge phase
    [ ] Implement fight phase
    [ ] Implement Command phase
    
[ ] Switch to hexes for world map instead of squares, details: https://www.redblobgames.com/grids/hexagons/
    * Likely want to model everything as a graph for many operations, e.g. path finding we get for free, each node could have a cost to traverse by team, terrain is tough for all, can pass through friends, not enemies

# QoL:
[ ] Implement highlight for the selected unit
[ ] Add in a log of what actions were taken

# Game data
[ ] Create space marine starter army
    * https://www.dakkadakka.com/wiki/en/Starter_Army_Lists_for_each_40k_Race_with_Costs%21#500_Pts._11
[ ] Create necron starter army




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


