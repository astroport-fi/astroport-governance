use anyhow::anyhow;
use cosmwasm_schema::serde::de::DeserializeOwned;
use cosmwasm_std::{
    coin, Addr, Api, BankMsg, Binary, BlockInfo, CustomMsg, CustomQuery, Empty, Querier, Storage,
    SubMsgResponse,
};
use cw_multi_test::error::AnyResult;
use cw_multi_test::{
    AppResponse, BankSudo, CosmosRouter, Module, Stargate, StargateMsg, StargateQuery,
};
use osmosis_std::types::osmosis::tokenfactory::v1beta1::{
    MsgBurn, MsgCreateDenom, MsgCreateDenomResponse, MsgMint, MsgSetBeforeSendHook,
    MsgSetDenomMetadata,
};

#[derive(Default)]
pub struct StargateModule;

impl Stargate for StargateModule {}

impl Module for StargateModule {
    type ExecT = StargateMsg;
    type QueryT = StargateQuery;
    type SudoT = Empty;

    fn execute<ExecC, QueryC>(
        &self,
        api: &dyn Api,
        storage: &mut dyn Storage,
        router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        block: &BlockInfo,
        sender: Addr,
        msg: Self::ExecT,
    ) -> AnyResult<AppResponse>
    where
        ExecC: CustomMsg + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        match msg.type_url.as_str() {
            MsgCreateDenom::TYPE_URL => {
                let tf_msg: MsgCreateDenom = msg.value.try_into()?;
                let submsg_response = SubMsgResponse {
                    events: vec![],
                    data: Some(
                        MsgCreateDenomResponse {
                            new_token_denom: format!(
                                "factory/{}/{}",
                                tf_msg.sender, tf_msg.subdenom
                            ),
                        }
                        .into(),
                    ),
                };
                Ok(submsg_response.into())
            }
            MsgMint::TYPE_URL => {
                let tf_msg: MsgMint = msg.value.try_into()?;
                let mint_coins = tf_msg
                    .amount
                    .expect("Empty amount in tokenfactory MsgMint!");
                let cw_coin = coin(mint_coins.amount.parse()?, mint_coins.denom);
                let bank_sudo = BankSudo::Mint {
                    to_address: tf_msg.mint_to_address.clone(),
                    amount: vec![cw_coin.clone()],
                };

                router.sudo(api, storage, block, bank_sudo.into())
            }
            MsgBurn::TYPE_URL => {
                let tf_msg: MsgBurn = msg.value.try_into()?;
                let burn_coins = tf_msg
                    .amount
                    .expect("Empty amount in tokenfactory MsgBurn!");
                let cw_coin = coin(burn_coins.amount.parse()?, burn_coins.denom);
                let burn_msg = BankMsg::Burn {
                    amount: vec![cw_coin.clone()],
                };

                router.execute(
                    api,
                    storage,
                    block,
                    Addr::unchecked(&tf_msg.sender),
                    burn_msg.into(),
                )
            }
            MsgSetDenomMetadata::TYPE_URL => Ok(AppResponse::default()),
            MsgSetBeforeSendHook::TYPE_URL => Ok(AppResponse::default()),
            _ => Err(anyhow!(
                "Unexpected exec msg {} from {sender:?}",
                msg.type_url
            )),
        }
    }

    fn query(
        &self,
        _api: &dyn Api,
        _storage: &dyn Storage,
        _querier: &dyn Querier,
        _block: &BlockInfo,
        _request: Self::QueryT,
    ) -> AnyResult<Binary> {
        unimplemented!("Stargate queries are not implemented")
    }

    fn sudo<ExecC, QueryC>(
        &self,
        _api: &dyn Api,
        _storage: &mut dyn Storage,
        _router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        _msg: Self::SudoT,
    ) -> AnyResult<AppResponse>
    where
        ExecC: CustomMsg + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        unimplemented!("Stargate sudo is not implemented")
    }
}
