# Generator Controller

The Generator Controller allows vxASTRO holders to vote on changing `alloc_point`s in the Generator contract every 2 weeks. Note that the Controller contract uses the word "pool" when referring to LP tokens (generators) available in the Generator contract.

## InstantiateMsg

Initialize the contract with the initial owner, the addresses of the xvASTRO, the Generator and the Factory contracts
and the max amount of pools that can receive ASTRO emissions at the same time.

```json
{
  "owner": "wasm...",
  "escrow_addr": "wasm...",
  "generator_addr": "wasm...",
  "factory_addr": "wasm...",
  "pools_limit": 5
}
```

## ExecuteMsg

### `kick_blacklisted_voters`

Remove votes of voters that are blacklisted.

```json
{
  "kick_blacklisted_voters": {
    "blacklisted_voters": ["wasm...", "wasm..."]
  }
}
```

### `kick_unlocked_voters`

Remove votes of voters that have unlocked their vxASTRO.

```json
{
  "kick_unlocked_voters": {
    "unlocked_voters": ["wasm...", "wasm..."]
  }
}
```

### `update_config`

Sets various configuration parameters. Any of them can be omitted.

```json
{
  "update_config": {
    "blacklisted_voters_limit": 22,
    "main_pool": "wasm...",
    "main_pool_min_alloc": "0.3"
  }
}
```

### `vote`

Vote on pools that will start to get an ASTRO distribution in the current period. For example, assume an address has voting
power `100`. Then, following the example below, pools will receive voting power 10, 50, 40 respectively. Note that all values are scaled so they sum to 10,000.

```json
{
  "vote": {
    "votes": [
      [
        "wasm...",
        1000
      ],
      [
        "wasm...",
        5000
      ],
      [
        "wasm...",
        4000
      ]
    ]
  }
}
```

### `tune_pools`

Calculate voting power for all pools and apply new allocation points in generator contract.

```json
{
  "tune_pools": {}
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
    "owner": "wasm...",
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

### `update_whitelist`

Adds or removes lp tokens which are eligible to receive votes.

```json
{
  "update_whitelist": {
    "add": [
        "wasm...",
        "wasm..."
    ],
    "remove": [
      "wasm...",
      "wasm..."
    ]
  }
}
```

### `update_networks`

Adds or removes network mappings for tuning pools on remote chains.

```json
{
  "update_networks": {
    "add": [
        {
          "address_prefix": "wasm", 
          "generator_address": "wasm124tapgv8wsn5t3rv2cvywhxxxxxxxxx", 
          "ibc_channel": "channel-1"
        }
    ],
    "remove": [
      "wasm",
    ]
  }
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `user_info`

Request:

```json
{
  "user_info": {
    "user": "wasm..."
  }
}
```

Returns last user's voting parameters.

```json
{
  "user_info_response": {
    "vote_ts": 1234567,
    "voting_power": 100,
    "slope": 0,
    "lock_end": 0,
    "votes": [
      [
        "wasm...",
        1000
      ],
      [
        "wasm...",
        5000
      ],
      [
        "wasm...",
        4000
      ]
    ]
  }
}
```

### `tune_info`

Returns last tune information.

```json
{
  "tune_info_response": {
    "tune_ts": 1234567,
    "pool_alloc_points": [
      [
        "wasm...",
        4000
      ],
      [
        "wasm...",
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
    "pool_addr": "wasm..."
  }
}
```

Response:

```json
{
  "voted_pool_info_response": {
    "vxastro_amount": 1000,
    "slope": 0
  }
}
```

### `pool_info_at_period`

Returns pool voting parameters at specified period.

Request:

```json
{
  "pool_info_at_period": {
    "pool_addr": "wasm...",
    "period": 10
  }
}
```

Response:

```json
{
  "voted_pool_info_response": {
    "vxastro_amount": 1000,
    "slope": 0
  }
}
```

### `config`

Returns the contract's config.

```json
{
  "owner": "wasm...",
  "escrow_addr": "wasm...",
  "generator_addr": "wasm...",
  "factory_addr": "wasm...",
  "pools_limit": 5
}
```
