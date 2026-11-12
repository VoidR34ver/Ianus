Ianus: Mac test build
=====================

This is an unsigned test build, so macOS will be suspicious of it.

To open it:
  1. Unzip it (double-click the zip).
  2. Right-click "Ianus.command" and choose Open, then Open again in the
     dialog. (A plain double-click is blocked the first time.)
     If it says it cannot be opened, open Terminal in this folder and run:
         xattr -dr com.apple.quarantine . && ./Ianus.command

It opens the pathfinding demo room (rooms/chase.room): a river with a bridge,
a Nextbot that avoids water and a Wader that does not. To open another room:
         ./Ianus.command rooms/cellar.room

Keys:  F5 play / stop   F1 pause   F2 step one tick   F3 speed
       F4 hide / show panels   F6 replay   F7 record   F8 reload the room
Move with WASD or the arrow keys while playing.

In the right-hand panel, click an object (an agent shows a "Pathfinding"
section with its settings and route) and open "Pathfinding tools" to show
routes and tags, or to paint tags onto tiles while editing.

This build is from a branch under test. If something looks wrong, a screenshot
and what you clicked is the most useful thing to send back.
