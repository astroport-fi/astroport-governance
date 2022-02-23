# Astroport Escrow fee distributor

Distributes the commission between users for the locked period.

## InstantiateMsg

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

Claims the amount from Escrow fee distributor for transfer to the recipient. Fields are optional.

```json
{
  "claim": {
    "recipient": "terra..."
  }
}
```

### `claim_many`

Claims the amount from Escrow fee distributor for transfer to the receivers.

```json
{
  "claim": {
    "receivers": ["terra...", "terra..."]
  }
}
```

### `update_config`

Updates general settings. Fields are optional.

```json
{
  "claim": {
    "claim_many_limit": 2,
    "is_claim_disabled": false
  }
}
```

### `receive`

Receives a commission in the form of Astro, which will be distributed among users.

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

Creates an offer for a new owner. The validity period of the offer is set in the `expires_in` variable.

```json
{
  "propose_new_owner": {
    "owner": "terra...",
    "expires_in": 1234567
  }
}
```

### `drop_ownership_proposal`

Removes the existing offer for the new owner.

```json
{
  "drop_ownership_proposal": {}
}
```

### `claim_ownership`

Used to claim(approve) new owner proposal, thus changing contract's owner.

```json
{
  "claim_ownership": {}
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `config`

Returns the information about the escrow fee distributor contract

```json
{
  "config": {}
}
```

### `user_reward`

Returns a commission amount in the form of Astro for user at timestamp.

`timestamp` a day cursor in seconds.

```json
{
  "user_reward": {
    "user": "user1",
    "timestamp": 1645113644
  }
}
```

### `available_reward_per_week`

Returns the vector that contains the amount of commission per week.

`start_after` a day in seconds.
`limit` a number of weeks.

```json
{
  "available_reward_per_week": {
    "start_after": 1645015524,
    "limit": 3
  }
}
```