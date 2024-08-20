use cosmwasm_schema::write_api;

use astroport_governance::emissions_controller::msg::ExecuteMsg;
use astroport_governance::emissions_controller::outpost::{
    OutpostInstantiateMsg, OutpostMsg, QueryMsg,
};

fn main() {
    write_api! {
        instantiate: OutpostInstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg<OutpostMsg>
    }
}
