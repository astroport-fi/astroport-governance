use astroport_governance::escrow_fee_distributor::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
};
use cosmwasm_schema::write_api;

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
        migrate: MigrateMsg
    }
}
