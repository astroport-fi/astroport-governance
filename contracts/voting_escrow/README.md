# Voting escrow

The vxASTRO contract allows staking xASTRO to gain voting power. Voting power depends on the time the user locking for.
Maximum lock time is 2 years which equals to 2.5 coefficient. For example, if the user locks 100 xASTRO for 2 years he
gains 250 voting power. Voting power is linearly decreased by passed periods. One period equals to 1 week.

## InstantiateMsg

```json
{
  "owner": "terra...",
  "deposit_token_addr": "terra..."
}
```

## ExecuteMsg

### `receive`

Create new lock, extend current lock's amount or deposit on behalf other address.

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

Extend lock time by 1 week.

```json
{
  "extend_lock_time": {
    "time": 604800
  }
}
```

### `withdraw`

Withdraw whole amount of xASTRO if lock expired.

```json
{
  "withdraw": {}
}
```

### `propose_new_owner`

Creates a request to change ownership. The validity period of the offer is set in the `expires_in` variable.
Only contract owner can execute this method.

```json
{
  "propose_new_owner": {
    "owner": "terra...",
    "expires_in": 1234567
  }
}
```

### `drop_ownership_proposal`

Removes the existing offer for the new owner. Only contract owner can execute this method.

```json
{
  "drop_ownership_proposal": {}
}
```

### `claim_ownership`

Used to claim(approve) new owner proposal, thus changing contract's owner. Only contract owner can execute this method.

```json
{
  "claim_ownership": {}
}
```

### `update_blacklist`

Updates blacklist. Removes addresses given in 'remove_addrs' array and appends new addresses given in 'append_addrs'.
Only contract owner can execute this method.

```json
{
  "append_addrs": ["terra...", "terra...", "terra..."],
  "remove_addrs": ["terra...", "terra..."]
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `total_voting_power`

Returns total voting power at the current block period.

Response:
```json
{
  "voting_power_response": {
    "voting_power": 100
  }
}
```

### `user_voting_power`

Returns user's voting power at the current block period.

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

Returns total voting power at the specific time (in seconds).

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

Returns user's voting power at the specific time (in seconds).

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

Returns user's lock information.

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

Returns contract's config.

```json
{
  "config_response": {
    "owner": "terra...",
    "deposit_token_addr" : "terra..."
  }
}
```