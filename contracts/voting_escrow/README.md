# Vote Escrowed Staked ASTRO

The vxASTRO contract allows xASTRO token holders to stake their tokens in order to boost their governance power as well as the amount of ASTRO they can get from Generator emissions. Voting power is boosted according to how long someone locks their xASTRO for.

Maximum lock time is 2 years, which gives the maximum possible boost of 2.5. For example, if a token holder locks 100 xASTRO for 2 years, they
get 250 vxASTRO. Their vxASTRO balance then goes down every week for the next 2 years (unless they relock) until it reaches zero.

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

### `extend_lock_time`

An example of extending the lock time for a vxASTRO position by 1 week.

```json
{
  "extend_lock_time": {
    "time": 604800
  }
}
```

### `withdraw`

Withdraw the whole amount of xASTRO if the lock for a vxASTRO position expired.

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

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `total_voting_power`

Returns the total supply of vxASTRO at the current block.

```json
{
  "voting_power_response": {
    "voting_power": 100
  }
}
```

### `user_voting_power`

Returns a user's vxASTRO balance at the current block.

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
    "voting_power": 10
  }
}
```

### `total_voting_power_at`

Returns the total vxASTRO supply at a specific timestamp (in seconds).

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
    "voting_power": 10
  }
}
```

### `user_voting_power_at`

Returns the user's vxASTRO balance at a specific timestamp (in seconds).

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
    "voting_power": 10
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

### `blacklisted_holders`

Returns blacklisted holders.

```json
{
  "blacklisted_holders": {}
}
```
