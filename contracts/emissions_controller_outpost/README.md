# Emissions Controller (Outpost)

The Emissions Controller Outpost is a lightweight satellite for the main Emissions Controller located on the Hub.
For the vxASTRO staker perspective, this contract has the same API as the main Emissions Controller.
However, Outpost can't perform fine-grained sanity checks for voted LP tokens.
Users can vote up to five pools only once per epoch.
Once votes are cast they can't be changed until the next epoch.
The contract composes a special internal IBC message to the Hub with the user's vote.
If sanity checks passed on the Hub, the vote is accepted.
In case of IBC failure or timeouts, the user can try to vote again.

## Emissions Setting

This endpoint is meant to be called during IBC hook processing.
It might be a gas extensive transaction, thus Astroport devs must settle it with supporting relayer operators prior to
the vxASTRO launch.
The contract has a permissionless endpoint which allows setting ASTRO emissions in the incentives contract for the next
epoch.
It filters out invalid LP tokens, checks that schedules have >= 1 uASTRO per second, sets reward schedules, and
IBC sends leftover funds back to the Hub.
Contract call must supply the exact ASTRO amount contained in the schedules.

## Permissioned Emissions Setting

In case the chain (for example, Sei) doesn't support IBC hooks, emissions message from the Hub might end up with ASTRO
bridged to the chain but not distributed.
In that case, the contract owner can call this endpoint along with the emissions voting outcome (schedules) for this
specific chain.
Same as in permissionless endpoint, this endpoint performs sanity checks, sets reward schedules, and IBC sends leftover
funds back to the Hub.

## Governance voting

vxASTRO stakers are allowed to vote on registered governance proposals from the Hub.
Proposal registration sets proposal start time so contract knows user's voting power at that time.
Only Hub's Emissions Contrller can initiate proposal registration via IBC messages.