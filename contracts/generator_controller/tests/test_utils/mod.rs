use astroport_governance::utils::WEEK;
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use terra_multi_test::{AppBuilder, BankKeeper, TerraApp, TerraMock};

pub mod controller_helper;
pub mod escrow_helper;

pub fn mock_app() -> TerraApp {
    let env = mock_env();
    let api = MockApi::default();
    let bank = BankKeeper::new();
    let storage = MockStorage::new();
    let custom = TerraMock::luna_ust_case();

    AppBuilder::new()
        .with_api(api)
        .with_block(env.block)
        .with_bank(bank)
        .with_storage(storage)
        .with_custom(custom)
        .build()
}

pub trait TerraAppExtension {
    fn app_next_period(&mut self);
}

impl TerraAppExtension for TerraApp {
    fn app_next_period(&mut self) {
        self.update_block(|block| {
            block.time = block.time.plus_seconds(WEEK);
            block.height += 1
        });
    }
}
