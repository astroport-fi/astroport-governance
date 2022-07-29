# Voting Escrow Delegation 

Contract allows delegating voting power to other account in NFT format.

## InstantiateMsg

Initialize the contract with the initial owner and the address of the xASTRO token.

```json
{
  "owner": "terra...",
  "nft_code_id": 123,
  "voting_escrow_addr": "terra..."
}
```

## ExecuteMsg

### `create_delegation`

Delegates the voting power to another account, according to the specified parameters, in the form of an NFT token.

```json
{
  "create_delegation": {
    "percent": "50",
    "expire_time": 12345,
    "token_id": "123",
    "recipient": "terra..."
  }
}
```

### `extend_delegation`

Extends a previously created delegation by a new specified parameters.

```json
{
  "extend_delegation": {
    "percent": "50",
    "expire_time": 12345,
    "token_id": "123",
    "recipient": "terra..."
  }
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

### `update_config`

Updates contract parameters.

```json
{
  "new_voting_escrow": "terra..."
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `config`

Returns the contract's config.

```json
{
  "config": {}
}
```

### `adjusted_balance`

Returns an adjusted voting power balance after accounting for delegations at specified timestamp.

```json
{
  "adjusted_balance": {
    "account": "terra...",
    "timestamp": 1234
  }
}
```

### `already_delegated_vp`

Returns an amount of delegated voting power.

Request:

```json
{
  "already_delegated_vp": {
    "account": "terra...",
    "timestamp": 1234
  }
}
```