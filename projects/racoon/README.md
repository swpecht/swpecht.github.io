[ ] Have health display over the top of units
    [ ] this may help to determine how things should be setup -- will we have multiple sprites attached to an entity? How to organize things? -- we probably want multiple bars

[ ] Implement a grid

[ ] Implement path finding
    [ ] ...

[ ] Implement building walls
    [ ] ...


Game ideas
* Move units around
* Don't know where the enemy is attacking from
* Can set the "loadouts" for your troops, e.g. choose what units are in it


Design:

Units can be marked AI controlled -- this allows the AI to give them a GoalPos.
Units are then responsible for navigating to the GoalPos -- they know things like their velocity, turning speed, etc.

Spawn units using events? Will have the "real" unit, but then other systems, like the unit_render_system or the health_render_system could be kicked off from there?
This might lead to a lot of lookups by entity id -- may not be performant
