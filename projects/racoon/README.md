[ ] Implement visualizing the simulation
  [*] Show static representation using gizmos
  [ ] Implement movement on ticks
  [ ] Implement lerping to smooth the movement
[ ] Implement a basic ai using cfr or some similar algorithm -- should we implement the card platypus gamestate?


[ ] Implement building walls on click -- can use a sprite
    [ ] ...
[ ] Implement attacking walls -- entity realizes it can't path through, so instead it attacks what's in front of it? --AI should probably do this

[ ] Have health display over the top of units
    [ ] this may help to determine how things should be setup -- will we have multiple sprites attached to an entity? How to organize things? -- we probably want multiple bars

[ ] Implement a grid

[ ] Implement path finding
    [ ] ...


Game ideas
* Move units around
* Don't know where the enemy is attacking from
* Can set the "loadouts" for your troops, e.g. choose what units are in it

Simulation design:
Legal actions:
* move units
* have units attack

Each unit can have a move and attack action every turn
Each player locks their move and attack action for every unit -- then both are executed simultaneously -- can occupy the same space
Cannot have own units occupy the same space

State we care about for the istates:
* Stale
  * Unit type
  * Health
* Known
  * Unit type
  * Health

How actions are evaluations:
* Attack stage
* Move stage




Design:

Units can be marked AI controlled -- this allows the AI to give them a GoalPos.
Units are then responsible for navigating to the GoalPos -- they know things like their velocity, turning speed, etc.

Spawn units using events? Will have the "real" unit, but then other systems, like the unit_render_system or the health_render_system could be kicked off from there?
This might lead to a lot of lookups by entity id -- may not be performant
