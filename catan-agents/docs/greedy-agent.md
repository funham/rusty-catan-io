# Greedy agent

The greedy agent is a minimal planning horizon agent.

It tries to do as much as possible in the current move, always choosing the first available objective from its priority list. It does not plan several turns ahead, but it does try to make each immediate action useful.

## Priorities

The agent evaluates objectives in this order:

0. Use dev card
1. Build city
2. Build settlement
3. Build road
4. Buy dev card

## Move

At the start of a decision, the agent first checks whether it can play a development card. If it can, it plays one immediately.

After that, it tries to fulfill the highest-priority available objective:

1. Build a city if possible.
2. Build a settlement if possible.
3. Build a road if possible.
4. Buy a development card if possible.

When placing a road, it prefers a placement that increases the number of possible settlement spots.

If none of the objectives are possible, the agent tries to trade with the bank. If the trade succeeds, it repeats the whole cycle from the top. If no useful trade is available, it ends the move.

## Init

During initial placement, the agent tries to acquire as many resource types as possible while maximizing overall probability points.

For the second settlement, it prefers resources that were not acquired by the first settlement.

## Trading

When no immediate objective can be executed, the agent evaluates legal bank and port trades.

If exact search state is available, it chooses the trade that unlocks the highest-priority next objective: city, settlement, road, then development card. It ignores trades that the bank cannot pay. Without search state, it falls back to the first legal bank trade.

## Discard and robber choices

When discarding half its hand, the agent discards one card at a time and keeps the remaining hand as close as possible to its next greedy objective.

When moving the robber, human visibility mode blocks the hex with the strongest visible opponent production. Counting mode also considers exact known opponent resources and prefers hexes that can steal cards useful for the agent's next objective.

When choosing a player to rob, human visibility mode prefers the legal target with the highest visible VP and resource total. Counting mode prefers the legal target holding the most useful exact resources for the agent's current hand.
