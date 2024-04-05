# Outpost

The Outpost contract enables staking, unstaking, voting in governance as well as voting on vxASTRO emissions from any chain where the Outpost contract is deployed on. The Hub and Outpost contracts are designed to work together, connected over IBC channels.

The Outpost defines the following messages that can be received over IBC:

```rust
/// Defines the messages that can be sent from the Hub to an Outpost
#[cw_serde]
pub enum Outpost {
    /// Mint xASTRO tokens for the user
    MintXAstro { receiver: String, amount: Uint128 },
}
```

The Outpost is responsible for validation before sending data to the Hub. In a case such as voting, it will query the xASTRO contract for the user's holding at the time a proposal was added and submit that as the voting power.

The Outpost defines the following execute messages:

```rust
#[cw_serde]
pub enum ExecuteMsg {
    /// Receive a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// Update parameters in the Outpost contract. Only the owner is allowed to
    /// update the config
    UpdateConfig {
        /// The new Hub address
        hub_addr: Option<String>,
    },
    /// Cast a vote on an Assembly proposal from an Outpost
    CastAssemblyVote {
        /// The ID of the proposal to vote on
        proposal_id: u64,
        /// The vote choice
        vote: ProposalVoteOption,
    },
    /// Cast a vote during an emissions voting period
    CastEmissionsVote {
        /// The votes in the format (pool address, percent of voting power)
        votes: Vec<(String, u16)>,
    },
    /// Kick an unlocked voter's voting power from the Generator Controller lite
    KickUnlocked {
        /// The address of the user to kick
        user: Addr,
    },
    /// Withdraw stuck funds from the Hub in case of specific IBC failures
    WithdrawHubFunds {},
}
```

## Message details

**Receive xASTRO via a Cw20HookMsg message for unstaking**

To unstake xASTRO from an Outpost a user needs to send the xASTRO to the Outpost. Once received, the contract burns the xASTRO and informs the Hub to unstake the true xASTRO on the Hub and return the resulting ASTRO. Should the IBC transactions fail at any point, the funds are returned to the user.

The following needs to be executed on the Outpost xASTRO contract. `msg` in this case is the base64 of `{"unstake":{}}`

```json
{
    "send": {
        "contract": "wasm123", 
        "amount": "1000000", 
        "msg":"eyJ1bnN0YWtlIjp7fX0="
    }
}
```


**Update Config**

Update config allows the owner to set a new address for the Hub. Updating the Hub address will remove the known Hub channel and a new one will need to be established.

```json
{
    "update_config": {
        "hub_addr": "wasm123..."
    }
}
```

**Cast a governance vote in the Assembly**

In order to cast a vote we need to know the voting power of a user at the time the proposal was created. The contract will retrieve the proposal information if it doesn't have it cached locally before validating the xASTRO holdings and submitting the vote.

```json
{
    "cast_assembly_vote":{
        "proposal_id": 1, 
        "vote": "for"
    }
}
```

**Cast a vote on vxASTRO emissions**

During voting periods in vxASTRO a user can vote on where emissions should be directed. The contract will check the vxASTRO holdings of the user before submitting the vote.

```json
{
    "cast_emissions_vote": {
        "votes":[
            ["wasm123..pool...", 1000]
        ]
    }
}
```

**Kick an unlocked vxASTRO user**

When a user unlocks in vxASTRO their voting power is removed immediately. This call may only be made by the vxASTRO contract. Once called the unlock is sent to the Hub to execute on the Generator Controller on the Hub.

```json
{
    "kick_unlocked":{
        "user":"wasm123"
    }
}
```

**Withdraw funds from the Hub**

In cases where specific IBC messages failed (mostly due to timeouts) there could be a situation where the funds are "stuck" on the Hub chain. To allow users to withdraw these funds we hold it in the Hub contract. `WithdrawHubFunds` will submit a request for the funds from the Hub and the funds will be sent over the CW20-ICS20 bridge again, if the user had funds stuck.

```json
{
    "withdraw_hub_funds":{}
}
```