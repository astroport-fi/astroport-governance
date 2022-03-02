# vxASTRO Escrow Fee Distributor

Distribute ASTRO fees every period to vxASTRO stakers.

## InstantiateMsg

Instantiate the fee distributor contract with the ASTRO and vxASTRO token contracts as well as claim related parameters.

```json
{
  "owner": "terra...",
  "astro_token": "terra...",
  "voting_escrow": "terra...",
  "claim_many_limit": 7,
  "is_claim_disabled": false
}
```

## ExecuteMsg

### `claim`

Claims ASTRO rewards for one period and sends them to the recipient.

```json
{
  "claim": {
    "recipient": "terra..."
  }
}
```

### `claim_many`

Claims ASTRO rewards from multiple periods and sends them to the recipient.

```json
{
  "claim": {
    "receivers": ["terra...", "terra..."]
  }
}
```

### `update_config`

Update the contract configuration.

```json
{
  "claim": {
    "claim_many_limit": 2,
    "is_claim_disabled": false
  }
}
```

### `receive`

Receive ASTRO fees (from the Maker) and prepares them to be distributed pro-rata to current stakers.

```json
{
  "receive": {
    "sender": "terra...",
    "amount": "123",
    "msg": "<base64_encoded_json_string>"
  }
}
```

### `propose_new_owner`

Creates a proposal to change the contract owner. The validity period for the offer is set in the `expires_in` variable.

```json
{
  "propose_new_owner": {
    "owner": "terra...",
    "expires_in": 1234567
  }
}
```

### `drop_ownership_proposal`

Removes the existing proposal to change contract ownership.

```json
{
  "drop_ownership_proposal": {}
}
```

### `claim_ownership`

Claim contract ownership.

```json
{
  "claim_ownership": {}
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `config`

Returns the contract configuration.

```json
{
  "config": {}
}
```

### `user_reward`

Returns the amount of ASTRO rewards a user can claim at a specific timestamp. `timestamp` is in seconds.

```json
{
  "user_reward": {
    "user": "user1",
    "timestamp": 1645113644
  }
}
```

### `available_reward_per_week`

Returns a vector with total amounts of ASTRO distributed as rewards every week to stakers. `start_after` is a timestamp in seconds. `limit` is the amount of entries to return.

```json
{
  "available_reward_per_week": {
    "start_after": 1645015524,
    "limit": 3
  }
}
```
