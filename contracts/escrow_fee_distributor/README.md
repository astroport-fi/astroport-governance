# Astroport Escrow fee distributor

Distributes the commission between users for the locked period.

## InstantiateMsg

```json
{
  "owner": "terra...",
  "token": "terra...",
  "voting_escrow": "terra...",
  "emergency_return": "terra...",
  "start_time": 1333
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
    "max_limit_accounts_of_claim": 2,
    "checkpoint_token_enabled": false
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

### `checkpoint_token`

Calculates the total number of tokens to be distributed in a given week.

```json
{
  "checkpoint_token": {}
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

### `fetch_user_balance_by_timestamp`

Returns a commission amount in the form of Astro for user at timestamp

```json
{
  "fetch_user_balance_by_timestamp": {
    "user": "user1",
    "timestamp": 4567
  }
}
```

### `voting_supply_per_week`

Returns the vector that contains voting supply per week.
`start_after` is a day in seconds
`limit` is a number of weeks 

```json
{
  "voting_supply_per_week": {
    "start_after": 1645015524,
    "limit": 3
  }
}
```

### `fee_tokens_per_week`

Returns the vector that contains the amount of commission per week.
`start_after` is a day in seconds
`limit` is a number of weeks

```json
{
  "fee_tokens_per_week": {
    "start_after": 1645015524,
    "limit": 3
  }
}
```