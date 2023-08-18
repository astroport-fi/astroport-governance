# Hub

The Hub contract enables staking, unstaking, voting in governance as well as voting on vxASTRO emissions from any chain where the Outpost contract is deployed on. The Hub and Outpost contracts are designed to work together, connected over IBC channels.

The Hub defines the following messages that can be received over IBC:

```rust
/// Hub defines the messages that can be sent from an Outpost to the Hub
pub enum Hub {
    /// Queries the Assembly for a proposal by ID via the Hub
    QueryProposal {
        /// The ID of the proposal to query
        id: u64,
    },
    /// Cast a vote on an Assembly proposal
    CastAssemblyVote {
        /// The ID of the proposal to vote on
        proposal_id: u64,
        /// The address of the voter
        voter: Addr,
        /// The vote choice
        vote_option: ProposalVoteOption,
        /// The voting power held by the voter, in this case xASTRO holdings
        voting_power: Uint128,
    },
    /// Cast a vote during an emissions voting period
    CastEmissionsVote {
        /// The address of the voter
        voter: Addr,
        /// The voting power held by the voter, in this case vxASTRO  lite holdings
        voting_power: Uint128,
        /// The votes in the format (pool address, percent of voting power)
        votes: Vec<(String, u16)>,
    },
    /// Stake ASTRO tokens for xASTRO
    Stake {},
    /// Unstake xASTRO tokens for ASTRO
    Unstake {
        // The user requesting the unstake and that should receive it
        receiver: String,
        /// The amount of xASTRO to unstake
        amount: Uint128,
    },
    /// Kick an unlocked voter's voting power from the Generator Controller lite
    KickUnlockedVoter {
        /// The address of the voter to kick
        voter: Addr,
    },
    /// Withdraw stuck funds from the Hub in case of specific IBC failures
    WithdrawFunds {
        /// The address of the user to withdraw funds for
        user: Addr,
    },
}
```

The Hub is unable to verify the information it receives, such as xASTRO holdings on an Outpost. To prevent invalid data reaching the Hub, it is only allowed to receive messages from the Outpost contract which verifies the data before sending it.

The Hub defines the following execute messages:

```rust
pub enum ExecuteMsg {
    /// Receive a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// Update parameters in the Hub contract. Only the owner is allowed to
    /// update the config
    UpdateConfig {
        /// The new address of the Assembly on the Hub, ignored if None
        assembly_addr: Option<String>,
        /// The new address of the CW20-ICS20 contract on the Hub that
        /// supports memo handling, ignored if None
        cw20_ics20_addr: Option<String>,
    },
    /// Add a new Outpost to the Hub. Only allowed Outposts can send IBC messages
    AddOutpost {
        /// The remote contract address of the Outpost to add
        outpost_addr: String,
        /// The channel to use for CW20-ICS20 IBC transfers
        cw20_ics20_channel: String,
    },
    /// Remove an Outpost from the Hub
    RemoveOutpost {
        /// The remote contract address of the Outpost to remove
        outpost_addr: String,
    },
}
```

## Message details

**Receive ASTRO via a Cw20HookMsg message containing an OutpostMemo**

To stake ASTRO from an Outpost a user needs to send the ASTRO over IBC (via the CW20-ICS20 contract) to the Hub. Together with these tokens they need to provide a valid JSON memo indicating the action to take. Currently, only staking is supported.

Using a chain's CLI, the command looks as follows

```bash
wasmd tx ibc-transfer transfer transfer channel-1 cw20_ics20_contract address 2000ibc/81A0618D89A81E830D4D670650E674770DEFFE344DCE3EDF3F62A9E3A506C0B4 -- --from user --memo '{"stake": {}}'
```

Once the memo is interpreted and executed, the xASTRO is minted to the user on the Outpost.

**Update Config**

Update config allows the owner to set a new address for the Assembly and the CW20-ICS20 contracts

```json
{
    "update_config": {
        "assembly_addr": "wasm123...",
        "cw20_ics20_addr": "wasm456..."
    }
}
```

**Adding an Outpost**

Only Outposts listed in the contract are allowed to open IBC channels and send messages.

```json
{
    "add_outpost":{
        "outpost_addr": "wasm123...", 
        "cw20_ics20_channel": "ASTRO transfer channel in CW20-ICS20 contract"
    }
}
```

**Remove an Outpost**

Removing an Outpost will not close the IBC channels, but will block new messages sent from the Outpost

```json
{
    "remove_outpost":{
        "outpost_addr": "wasm123..."
    }
}
```