use crate::contract::{instantiate, query};
use astroport_governance::escrow_fee_distributor::{ConfigResponse, InstantiateMsg, QueryMsg};

use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{from_binary, Addr};

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner".to_string(),
        astro_token: "token".to_string(),
        voting_escrow_addr: "voting_escrow".to_string(),
        emergency_return_addr: "emergency_return".to_string(),
        start_time: 0,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &vec![]);
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        from_binary::<ConfigResponse>(&query(deps.as_ref(), env, QueryMsg::Config {}).unwrap())
            .unwrap(),
        ConfigResponse {
            owner: Addr::unchecked("owner"),
            astro_token: Addr::unchecked("token"),
            voting_escrow_addr: Addr::unchecked("voting_escrow"),
            emergency_return_addr: Addr::unchecked("emergency_return"),
            start_time: 0,
            last_token_time: 0,
            time_cursor: 0,
            max_limit_accounts_of_claim: 10,
            checkpoint_token_enabled: false
        }
    );
}
