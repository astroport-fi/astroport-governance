# Astroport Assembly

The Assembly contract allows ASTRO holders to post new on-chain proposals that can execute arbitrary logic and then vote on them.

## InstantiateMsg

```json
{
  "xastro_token_addr": "terra...",
  "staking_addr": "terra...",
  "proposal_voting_period": 123,
  "proposal_effective_delay": 123,
  "proposal_expiration_period": 123,
  "proposal_required_deposit": 123,
  "proposal_required_quorum": 12,
  "proposal_required_threshold": 12
}
```

## ExecuteMsg

### `receive`

Submitting proposal.

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

Casts vote for an active propose.

```json
{
  "cast_vote": {
    "proposal_id": 123,
    "vote": "for"
  }
}
```

### `end_proposal`

Ends proposal.

```json
{
  "end_proposal": {
    "proposal_id": 123
  }
}
```

### `execute_proposal`

Executes proposal messages

```json
{
  "execute_proposal": {
    "proposal_id": 123
  }
}
```

### `remove_completed_proposal`

Removes completed proposal in the proposal list.

```json
{
  "remove_completed_proposal": {
    "proposal_id": 123
  }
}
```

### `update_config`

Update current assembly contract. Only assembly contract via passed proposal can execute it.

```json
{
  "update_config": {
    "xastro_token_addr": "terra...",
    "staking_addr": "terra...",
    "proposal_voting_period": 123,
    "proposal_effective_delay": 123,
    "proposal_expiration_period": 123,
    "proposal_required_deposit": 123,
    "proposal_required_quorum": 12,
    "proposal_required_threshold": 12
  }
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `config`

Returns the information about the assembly contract

```json
{
  "config": {}
}
```

### `proposals`

Returns list of proposals

```json
{
  "proposals": {
    "start_after": 10,
    "limit": 10
  }
}
```

### `proposal`

Returns information about proposal

```json
{
  "proposal": {
    "proposal_id": 123
  }
}
```

### `proposal_votes`

Returns information about proposal votes

```json
{
  "proposal_votes": {
    "proposal_id": 123
  }
}
```