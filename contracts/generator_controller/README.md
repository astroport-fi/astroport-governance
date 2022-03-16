# GeneratorController contract

The GeneratorController contract allows voting for pools using vxASTRO amount.

## InstantiateMsg

Initialize the contract with the initial owner, the addresses of the xvASTRO, the Generator and the Factory contracts
and limit of pools which can receive allocation points.

```json
{
  "owner": "terra...",
  "escrow_addr": "terra...",
  "generator_addr": "terra...",
  "factory_addr": "terra...",
  "pools_limit": 5
}
```

## ExecuteMsg

### `vote`

Cast votes for the next period for pools according to specified basic points distribution. For example, user has voting
power 100. Then, following the example below, pools will receive voting power 10, 50, 40 respectively. Note: The sum of
all basic points should be within [0, 10000] limit.

```json
{
  "vote": {
    "votes": [
      [
        "terra...",
        1000
      ],
      [
        "terra...",
        5000
      ],
      [
        "terra...",
        4000
      ]
    ]
  }
}
```

### `gauge_pools`

Only contract owner can call this function. Calculate voting power for all pools and apply new allocation points in
generator contract.

```json
{
  "gauge_pools": {}
}
```

### `change_pool_limit`

Only contract owner can call this function. Change max number of pools that can receive an ASTRO allocation.

```json
{
  "change_pool_limit": {
    "limit": 6
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

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `user_info`

Request:

```json
{
  "user_info": {
    "user": "terra..."
  }
}
```

Returns last user's voting parameters.

```json
{
  "user_info_response": {
    "vote_ts": 1234567,
    "voting_power": 100,
    "slope": 15.45,
    "lock_end": 10,
    "votes": [
      [
        "terra...",
        1000
      ],
      [
        "terra...",
        5000
      ],
      [
        "terra...",
        4000
      ]
    ]
  }
}
```

### `gauge_info`

Returns last gauge information.

```json
{
  "gauge_info_response": {
    "gauge_ts": 1234567,
    "pool_alloc_points": [
      [
        "terra...",
        4000
      ],
      [
        "terra...",
        6000
      ]
    ]
  }
}
```

### `pool_info`

Returns pool voting parameters at the current block period.

Request:

```json
{
  "pool_info": {
    "pool_addr": "terra..."
  }
}
```

Response:

```json
{
  "voted_pool_info_response": {
    "vxastro_amount": 1000,
    "slope": 10.2
  }
}
```

### `pool_info_at_period`

Returns pool voting parameters at specified period.

Request:

```json
{
  "pool_info_at_period": {
    "pool_addr": "terra...",
    "period": 10
  }
}
```

Response:

```json
{
  "voted_pool_info_response": {
    "vxastro_amount": 1000,
    "slope": 10.2
  }
}
```

### `config`

Returns the contract's config.

```json
{
  "owner": "terra...",
  "escrow_addr": "terra...",
  "generator_addr": "terra...",
  "factory_addr": "terra...",
  "pools_limit": 5
}
```
