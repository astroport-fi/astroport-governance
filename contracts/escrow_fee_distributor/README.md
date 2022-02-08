# Astroport Escrow fee distributor


---

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
    "recipient": "terra...",
    "amount": "123"
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

### `astro_recipients_per_week`

Returns the list of accounts who will get ASTRO fees every week

```json
{
  "astro_recipients_per_week": {}
}
```