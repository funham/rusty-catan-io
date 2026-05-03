# Greedy agent

Minimal planning horizon agent. Tries to do as much as possible in one move, targeting different objectives accordint to it's internal priorities.

---

#### Priorities:
 0. Use dev card
 1. Build city
 2. Build settlement
 3. Build road
 4. Buy dev

---

#### Move:

If has an opportunity to play a dev card - plays it immidiately.

If it's possible to fulfill any of the listed objectives - does it immediately.

When placing a road, tries to do it in a way that increases the amount of possible settlement placement spots.

If none of this is possible, than tries to trade with a bank. If succseds then repeats the whole cycle. Otherwise ends the move.

---

#### Init:

In the init stage tries to acquire as much resources with the best overall probability points as possible. When placing the second settlement targets only resources that were not aquired by the first settlement.

---

#### Trading:

#TODO