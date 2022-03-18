# Astral Assembly

The Assembly contract allows xASTRO and vxASTRO holders as well as Initial Astroport Builders to post and vote on new on-chain proposals that can execute arbitrary logic.

## InstantiateMsg

Instantiate the contract with proposal parameter limitations and the xASTRO and builder unlock contract addresses.

```json
{
  "xastro_token_addr": "terra...",
  "builder_unlock_addr": "terra...",
  "proposal_voting_period": 123,
  "proposal_effective_delay": 123,
  "proposal_expiration_period": 123,
  "proposal_required_deposit": "123",
  "proposal_required_quorum": "0.55",
  "proposal_required_threshold": "0.55"
}
```

## ExecuteMsg

### `receive`

Submit a new on-chain proposal.

```json
{
  "receive": {
    "sender": "terra...",
    "amount": "123",
    "msg": "<base64_encoded_json_string>"
  }
}
```

### `cast_vote`

Casts a vote for an active proposal.

```json
{
  "cast_vote": {
    "proposal_id": 123,
    "vote": "for"
  }
}
```

### `end_proposal`

Ends an expired proposal.

```json
{
  "end_proposal": {
    "proposal_id": 123
  }
}
```

### `execute_proposal`

Executes a proposal.

```json
{
  "execute_proposal": {
    "proposal_id": 123
  }
}
```

### `remove_completed_proposal`

Removes a completed proposal from the proposal list.

```json
{
  "remove_completed_proposal": {
    "proposal_id": 123
  }
}
```

### `update_config`

Update contract parameters. Only the Assembly is allowed to update its own parameters.

```json
{
  "update_config": {
    "xastro_token_addr": "terra...",
    "builder_unlock_addr": "terra...",
    "proposal_voting_period": 123,
    "proposal_effective_delay": 123,
    "proposal_expiration_period": 123,
    "proposal_required_deposit": "123",
    "proposal_required_quorum": "0.55",
    "proposal_required_threshold": "0.55"
  }
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `config`

Returns Astral Assembly parameters.

```json
{
  "config": {}
}
```

### `proposals`

Returns the current proposal list.

```json
{
  "proposals": {
    "start_after": 10,
    "limit": 10
  }
}
```

### `proposal`

Returns information about a specific proposal.

```json
{
  "proposal": {
    "proposal_id": 123
  }
}
```

### `proposal_votes`

Returns information about the votes cast on a proposal.

```json
{
  "proposal_votes": {
    "proposal_id": 123
  }
}
```

### `user_voting_power`

Returns voting power of the given user.

```json
{
  "user_voting_power": {
    "user": "terra..."
  }
}
```
