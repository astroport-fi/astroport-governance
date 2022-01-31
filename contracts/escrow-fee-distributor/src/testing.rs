use crate::contract::{instantiate, query};
use astroport_governance::escrow_fee_distributor::{ConfigResponse, InstantiateMsg, QueryMsg};

use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{from_binary, Addr};

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner".to_string(),
        token: "token".to_string(),
        voting_escrow: "voting_escrow".to_string(),
        emergency_return: "emergency_return".to_string(),
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
            token: Addr::unchecked("token"),
            voting_escrow: Addr::unchecked("voting_escrow"),
            emergency_return: Addr::unchecked("emergency_return"),
            start_time: 0,
            last_token_time: 0,
            time_cursor: 0,
            can_checkpoint_token: false,
            is_killed: false
        }
    );
}
