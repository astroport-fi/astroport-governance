use cosmwasm_schema::write_api;

use astroport_governance::emissions_controller::hub::{HubInstantiateMsg, HubMsg, QueryMsg};
use astroport_governance::emissions_controller::msg::ExecuteMsg;

fn main() {
    write_api! {
        instantiate: HubInstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg<HubMsg>
    }
}
