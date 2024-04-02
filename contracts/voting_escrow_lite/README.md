# Vote Escrowed Staked ASTRO Lite

The vxASTRO lite contract allows xASTRO token holders to lock their tokens in order to gain emissions voting power that
is used in voting which pools should be receiving ASTRO emissions.

The xASTRO is lock forever, or until a holder decides to unlock the position. Unlocking is subject to a 2 week (default)
waiting period until withdrawal is allowed. Once an unlocking period starts, the holder will lose the emissions voting power
immediately.

## InstantiateMsg

Initialize the contract with the initial owner and the address of the xASTRO token.

```json
{
  "owner": "terra...",
  "deposit_token_addr": "terra..."
}
```

## ExecuteMsg

### `receive`

Create new lock/vxASTRO position, deposit more xASTRO in the user's vxASTRO position or deposit on behalf of another address.

```json
{
  "receive": {
    "sender": "terra...",
    "amount": "123",
    "msg": "<base64_encoded_json_string>"
  }
}
```

### `unlock`

Unlock the whole position in vxASTRO, subject to a waiting period until `withdraw` is possible

```json
{
  "unlock": {}
}
```

### `withdraw`

Withdraw the whole amount of xASTRO if the lock for a vxASTRO position has been unlocked and the waiting period has passed.

```json
{
  "withdraw": {}
}
```

### `propose_new_owner`

Create a request to change contract ownership. The validity period of the offer is set by the `expires_in` variable.
Only the current contract owner can execute this method.

```json
{
  "propose_new_owner": {
    "owner": "terra...",
    "expires_in": 1234567
  }
}
```

### `drop_ownership_proposal`

Delete the contract ownership transfer proposal. Only the current contract owner can execute this method.

```json
{
  "drop_ownership_proposal": {}
}
```

### `claim_ownership`

Used to claim contract ownership. Only the newly proposed contract owner can execute this method.

```json
{
  "claim_ownership": {}
}
```

### `update_blacklist`

Updates the list of addresses that are prohibited from staking in vxASTRO or if they are already staked, from voting with their vxASTRO in the Astral Assembly. Only the contract owner can execute this method.

```json
{
  "append_addrs": ["terra...", "terra...", "terra..."],
  "remove_addrs": ["terra...", "terra..."]
}
```

### `update_config`

Updates contract parameters.

```json
{
  "new_guardian": "terra..."
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `total_voting_power`

Returns the total supply of vxASTRO at the current block, for this version, will always return 0.

```json
{
  "voting_power_response": {
    "voting_power": 0
  }
}
```

### `user_voting_power`

Returns a user's vxASTRO balance at the current block, for this version, will always return 0.

Request:

```json
{
  "user_voting_power": {
    "user": "terra..."
  }
}
```

Response:

```json
{
  "voting_power_response": {
    "voting_power": 0
  }
}
```

### `total_voting_power_at`

Returns the total vxASTRO supply at a specific timestamp (in seconds), for this version, will always return 0.

Request:

```json
{
  "total_voting_power_at": {
    "time": 1234567
  }
}
```

Response:

```json
{
  "voting_power_response": {
    "voting_power": 0
  }
}
```

### `user_voting_power_at`

Returns the user's vxASTRO balance at a specific timestamp (in seconds), for this version, will always return 0.

Request:

```json
{
  "user_voting_power_at": {
    "user": "terra...",
    "time": 1234567
  }
}
```

Response:

```json
{
  "voting_power_response": {
    "voting_power": 0
  }
}
```

### `total_emissions_voting_power`

Returns the total emissions voting power of vxASTRO at the current block.

```json
{
  "voting_power_response": {
    "voting_power": 0
  }
}
```

### `user_emissions_voting_power`

Returns a user's emissions voting power at the current block.

Request:

```json
{
  "user_emissions_voting_power": {
    "user": "terra..."
  }
}
```

Response:

```json
{
  "voting_power_response": {
    "voting_power": 0
  }
}
```

### `total_emissions_voting_power_at`

Returns the total emissions voting power at a specific timestamp (in seconds), for this version, will always return 0.

Request:

```json
{
  "total_emissions_voting_power_at": {
    "time": 1234567
  }
}
```

Response:

```json
{
  "voting_power_response": {
    "voting_power": 0
  }
}
```

### `user_emissions_voting_power_at`

Returns a user's emissions voting power at a specific timestamp (in seconds), for this version, will always return 0.

Request:

```json
{
  "user_emissions_voting_power_at": {
    "user": "terra...",
    "time": 1234567
  }
}
```

Response:

```json
{
  "voting_power_response": {
    "voting_power": 0
  }
}
```
### `lock_info`

Returns the information about a user's vxASTRO position.

Request:

```json
{
  "lock_info": {
    "user": "terra..."
  }
}
```

Response:

```json
{
  "lock_info_response": {
    "amount": 10,
    "coefficient": 2.5,
    "start": 2600,
    "end": 2704
  }
}
```

### `config`

Returns the contract's config.

```json
{
  "config_response": {
    "owner": "terra...",
    "deposit_token_addr" : "terra..."
  }
}
```

### `blacklisted_voters`

Returns blacklisted voters.

```json
{
  "blacklisted_voters": {
    "start_after": "terra...",
    "limit": 5
  }
}
```

### `check_voters_are_blacklisted`

Checks if specified addresses are blacklisted

```json
{
  "check_voters_are_blacklisted": {
    "voters": ["terra...", "terra..."]
  }
}
```