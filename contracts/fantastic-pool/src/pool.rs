use astroport::asset::AssetInfo;
use astroport::router::{ExecuteMsg as AstroportRouterExecuteMsg, SwapOperation};
use cosmwasm_std::{
    to_binary, Addr, BankMsg, Coin, CosmosMsg, Env, QuerierWrapper, Response, StdResult, Storage,
    Uint128, WasmMsg,
};
use cw20::{Cw20CoinVerified, Cw20ExecuteMsg};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::epoch::EPOCH;
use crate::helpers::Unit;
use crate::msg::{CalcRedeemResult, ExecuteMsg};
use crate::oracle::{SHARE_ORACLE, SYNTH_ORACLE};
use crate::{msg::CalcMintResult, ContractError};

const ASTROPORT_ROUTER: &str = "terra13wf295fj9u209nknz2cgqmmna7ry3d3j5kv7t4";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolConfig {
    /// denom of collateral token, eg: uluna
    pub collateral_denom: String,
    /// address of the synth token
    pub synth: Addr,
    pub share: Addr,

    /// part of synth collaterized, used for minting/redeeming
    pub collateral_ratio: Uint128,
    pub min_collateral_ratio: Uint128,
    pub last_refresh_collateral_ratio: u64,
    pub refresh_collateral_ratio_cooldown: u64,
    pub collateral_ratio_step: Uint128,
    pub price_band: Uint128,

    /// fee charged in collateral
    pub minting_fee: Uint128,
    pub redemption_fee: Uint128,

    pub total_fee: Uint128,
    pub total_unclaimed_synth: Uint128,
    pub total_unclaimed_collateral: Uint128,
    pub total_unclaimed_share: Uint128,

    pub mint_paused: bool,
    pub redeem_paused: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct UserInfo {
    /// last block number when user mint or redeem
    pub last_action_block: u64,
    pub synth_balance: Uint128,
    pub share_balance: Uint128,
    pub collateral_balance: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CollectResult {
    pub collateral_amount: Uint128,
    pub synth_amount: Uint128,
    pub share_amount: Uint128,
}

impl PoolConfig {
    pub fn init(collateral_denom: String) -> Self {
        PoolConfig {
            collateral_denom,
            synth: Addr::unchecked(""),
            share: Addr::unchecked(""),
            collateral_ratio: Unit::precision(),
            min_collateral_ratio: Unit::precision(),
            last_refresh_collateral_ratio: 0,
            refresh_collateral_ratio_cooldown: 600,
            collateral_ratio_step: 2500u128.into(),
            price_band: 5000u128.into(),
            minting_fee: Uint128::from(3000u128),
            redemption_fee: Uint128::from(5000u128),
            total_fee: Uint128::zero(),
            total_unclaimed_synth: Uint128::zero(),
            total_unclaimed_collateral: Uint128::zero(),
            total_unclaimed_share: Uint128::zero(),
            mint_paused: false,
            redeem_paused: false,
        }
    }

    pub fn calc_mint(&self, collateral_amount: Uint128) -> CalcMintResult {
        let precision = Unit::precision();
        let buy_share_value = collateral_amount * (precision - self.collateral_ratio) / precision;
        let synth_out = collateral_amount * (precision - self.minting_fee) / precision;
        let fee =
            collateral_amount * self.collateral_ratio * self.minting_fee / precision / precision;
        CalcMintResult {
            synth_out,
            buy_share_value,
            fee,
        }
    }

    /// return collateral amount, share amount, and fee amount
    pub fn calc_redeem(&self, synth_amount: Uint128, share_price: Uint128) -> CalcRedeemResult {
        let precision = Unit::precision();
        let collateral_out =
            synth_amount * self.collateral_ratio * (precision - self.redemption_fee)
                / precision
                / precision;
        let fee = synth_amount * self.redemption_fee / precision;

        let share_out =
            synth_amount * (precision - self.redemption_fee) * (precision - self.collateral_ratio)
                / share_price
                / precision;

        CalcRedeemResult {
            collateral_out,
            share_out,
            fee,
        }
    }

    pub fn mint(
        &mut self,
        user: &mut UserInfo,
        block_height: u64,
        collateral_amount: Uint128,
    ) -> CalcMintResult {
        let result = self.calc_mint(collateral_amount);
        self.total_unclaimed_synth += result.synth_out;
        self.total_fee += result.fee;
        user.last_action_block = block_height;
        user.synth_balance += result.synth_out;
        result
    }

    pub fn redeem(
        &mut self,
        user: &mut UserInfo,
        block_height: u64,
        synth_amount: Uint128,
        share_price: Uint128,
    ) -> CalcRedeemResult {
        let result = self.calc_redeem(synth_amount, share_price);
        self.total_unclaimed_collateral += result.collateral_out;
        self.total_unclaimed_share += result.share_out;
        self.total_fee += result.fee;
        user.last_action_block = block_height;
        user.share_balance = user.share_balance + result.share_out;
        user.collateral_balance = user.collateral_balance + result.collateral_out;
        result
    }

    pub fn collect(&mut self, user: &mut UserInfo) -> CollectResult {
        let synth_amount = user.synth_balance;
        let share_amount = user.share_balance;
        let collateral_amount = user.collateral_balance;
        self.total_unclaimed_collateral -= collateral_amount;
        self.total_unclaimed_share -= share_amount;
        self.total_unclaimed_synth -= synth_amount;
        user.collateral_balance = Uint128::zero();
        user.share_balance = Uint128::zero();
        user.synth_balance = Uint128::zero();

        CollectResult {
            collateral_amount,
            synth_amount,
            share_amount,
        }
    }

    pub fn refresh_collateral_ratio(&mut self, synth_twap: Uint128) {
        let mut collateral_ratio = self.collateral_ratio;
        let target_price = Unit::precision();
        let max_collateral_ratio = Unit::precision();

        if synth_twap > target_price + self.price_band {
            collateral_ratio = collateral_ratio - self.collateral_ratio_step
        } else if synth_twap < target_price - self.price_band {
            collateral_ratio = collateral_ratio + self.collateral_ratio_step
        }

        self.collateral_ratio =
            collateral_ratio.clamp(self.min_collateral_ratio, max_collateral_ratio);
    }
}

pub struct Pool<'a> {
    pub pool: Item<'a, PoolConfig>,
    pub user: Map<'a, &'a Addr, UserInfo>,
}

impl<'a> Pool<'a> {
    pub const fn new() -> Self {
        Self {
            pool: Item::new("pool"),
            user: Map::new("user"),
        }
    }

    /// initialize pool state with default config value
    pub fn initialize(
        &self,
        storage: &mut dyn Storage,
        collateral_denom: String,
    ) -> Result<(), ContractError> {
        let pool = PoolConfig::init(collateral_denom);
        self.pool.save(storage, &pool)?;
        Ok(())
    }

    pub fn mint(
        &self,
        storage: &mut dyn Storage,
        querier: &QuerierWrapper,
        env: Env,
        sender: &Addr,
        funds: Vec<Coin>,
        min_synth_out: Uint128,
    ) -> Result<Response, ContractError> {
        let block_height = env.block.height;
        let mut pool = self.get_pool(storage)?;
        let mut user = self.get_user(storage, sender)?;
        if pool.mint_paused {
            return Err(ContractError::MintingPaused {});
        }

        let collateral_in = funds
            .iter()
            .find(|&x| x.denom == pool.collateral_denom) // coin = {denom, amount} // ulunu
            .map(|x| x.amount)
            .unwrap_or(Uint128::zero());

        if collateral_in.is_zero() {
            return Err(ContractError::MintInvalidCollateralAmount {});
        }

        let CalcMintResult {
            synth_out,
            buy_share_value,
            fee,
        } = pool.mint(&mut user, block_height, collateral_in);

        if synth_out < min_synth_out {
            return Err(ContractError::SlippageReached {});
        }

        EPOCH.assert_mint_amount(storage, querier, &pool, synth_out, env.block.time.seconds())?;

        self.user.save(storage, sender, &user)?;
        self.pool.save(storage, &pool)?;

        let msgs = Pool::buy_share_and_burn(&env.contract.address, &pool, buy_share_value)?;
        Ok(Response::new()
            .add_attribute("action", "mint")
            .add_attribute("input", collateral_in)
            .add_attribute("output", synth_out)
            .add_attribute("fee", fee)
            .add_attribute("buy_share_value", buy_share_value)
            .add_messages(msgs))
    }

    fn buy_share_and_burn(
        this_addr: &Addr,
        pool: &PoolConfig,
        buy_share_value: Uint128,
    ) -> StdResult<Vec<WasmMsg>> {
        let mut messages: Vec<WasmMsg> = vec![];
        if !buy_share_value.is_zero() {
            let denom = pool.collateral_denom.clone();
            messages.push(WasmMsg::Execute {
                contract_addr: ASTROPORT_ROUTER.into(),
                msg: to_binary(&AstroportRouterExecuteMsg::ExecuteSwapOperations {
                    operations: vec![SwapOperation::AstroSwap {
                        offer_asset_info: AssetInfo::NativeToken {
                            denom: pool.collateral_denom.clone(),
                        },
                        ask_asset_info: AssetInfo::Token {
                            contract_addr: pool.share.clone(),
                        },
                    }],
                    minimum_receive: None,
                    to: Some(this_addr.clone()),
                })?,
                funds: vec![Coin {
                    denom,
                    amount: buy_share_value,
                }],
            });
            messages.push(WasmMsg::Execute {
                contract_addr: this_addr.to_string(),
                msg: to_binary(&ExecuteMsg::BurnShare {})?,
                funds: vec![],
            });
        }
        Ok(messages)
    }

    pub fn redeem(
        &self,
        storage: &mut dyn Storage,
        querier: &QuerierWrapper,
        env: Env,
        sender: &Addr,
        synth_input: Cw20CoinVerified,
        min_collateral_out: Uint128,
        min_share_out: Uint128,
    ) -> Result<Response, ContractError> {
        let mut pool = self.get_pool(storage)?;
        let mut user = self.get_user(storage, sender)?;

        if pool.redeem_paused {
            return Err(ContractError::RedemptionPaused {});
        }

        // important! user can send fake token to trigger this
        if pool.synth != synth_input.address {
            return Err(ContractError::RedeemInvalidSynthInput {
                want: pool.synth,
                send: synth_input.address,
            });
        }

        let synth_amount = synth_input.amount;
        if synth_amount.is_zero() {
            return Err(ContractError::RedeemEmptyAmount {});
        }
        let share_price = SHARE_ORACLE.get_spot_price(storage, &querier)?;

        let CalcRedeemResult {
            collateral_out,
            share_out,
            fee,
        } = pool.redeem(&mut user, env.block.height, synth_amount, share_price);

        if collateral_out < min_collateral_out || share_out < min_share_out {
            return Err(ContractError::SlippageReached {});
        }

        self.pool.save(storage, &pool)?;
        self.user.save(storage, sender, &user)?;

        Ok(Response::new()
            .add_attribute("action", "redeem")
            .add_attribute("input", synth_amount)
            .add_attribute("share_out", share_out)
            .add_attribute("collateral_out", collateral_out)
            .add_attribute("fee", fee))
    }

    pub fn collect(
        &self,
        storage: &mut dyn Storage,
        env: Env,
        sender: &Addr,
    ) -> Result<Response, ContractError> {
        let mut pool = self.get_pool(storage)?;
        let mut user = self.get_user(storage, sender)?;

        if env.block.height <= user.last_action_block {
            return Err(ContractError::CollectTooEarly {});
        }

        let CollectResult {
            share_amount,
            synth_amount,
            collateral_amount,
        } = pool.collect(&mut user);
        self.user.save(storage, &sender, &user)?;
        self.pool.save(storage, &pool)?;

        // send tokens
        let mut messages: Vec<CosmosMsg> = vec![];

        if !synth_amount.is_zero() {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: pool.synth.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Mint {
                    recipient: sender.clone().to_string(),
                    amount: synth_amount,
                })?,
                funds: vec![],
            }))
        }

        if !share_amount.is_zero() {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: pool.share.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Mint {
                    recipient: sender.clone().to_string(),
                    amount: share_amount,
                })?,
                funds: vec![],
            }))
        }

        if !collateral_amount.is_zero() {
            messages.push(CosmosMsg::Bank(BankMsg::Send {
                to_address: sender.to_string(),
                amount: vec![Coin::new(collateral_amount.into(), &pool.collateral_denom)],
            }))
        }

        Ok(Response::new()
            .add_attribute("action", "collect")
            .add_attribute("collateral_amount", collateral_amount)
            .add_attribute("share_amount", share_amount)
            .add_attribute("synth_amount", synth_amount)
            .add_messages(messages))
    }

    pub fn refresh_collateral_ratio(
        &self,
        storage: &mut dyn Storage,
        env: Env,
    ) -> Result<Response, ContractError> {
        let mut pool = self.get_pool(storage)?;

        let now = env.block.time.seconds();
        if now < pool.last_refresh_collateral_ratio + pool.refresh_collateral_ratio_cooldown {
            return Err(ContractError::CollateralRatioRefreshCooldown {});
        }

        let (synth_twap, last_twap_update) = SYNTH_ORACLE.get_twap(storage)?;
        if last_twap_update < pool.last_refresh_collateral_ratio {
            return Err(ContractError::PriceUnavailableOrOutdated {});
        }

        pool.refresh_collateral_ratio(synth_twap);
        self.pool.save(storage, &pool)?;

        Ok(Response::new()
            .add_attribute("action", "refresh_collateral_ratio")
            .add_attribute("collateral_ratio", pool.collateral_ratio)
            .add_attribute("timestamp", now.to_string()))
    }

    // ======== Admin function ========
    pub fn set_fee(
        &self,
        storage: &mut dyn Storage,
        minting_fee: Uint128,
        redemption_fee: Uint128,
    ) -> Result<Response, ContractError> {
        self.pool
            .update(storage, |mut state| -> Result<_, ContractError> {
                state.minting_fee = minting_fee;
                state.redemption_fee = redemption_fee;
                Ok(state)
            })?;

        Ok(Response::new()
            .add_attribute("action", "set_fee")
            .add_attribute("minting_fee", minting_fee)
            .add_attribute("redemption_fee", redemption_fee))
    }

    pub fn toggle(
        &self,
        storage: &mut dyn Storage,
        mint_paused: bool,
        redeem_paused: bool,
    ) -> Result<Response, ContractError> {
        self.pool
            .update(storage, |mut state| -> Result<_, ContractError> {
                state.mint_paused = mint_paused;
                state.redeem_paused = redeem_paused;
                Ok(state)
            })?;

        Ok(Response::new()
            .add_attribute("action", "transfer_ownership")
            .add_attribute("mint_paused", mint_paused.to_string())
            .add_attribute("redeem_paused", redeem_paused.to_string()))
    }

    pub fn set_min_collateral_ratio(
        &self,
        storage: &mut dyn Storage,
        min_collateral_ratio: Uint128,
    ) -> Result<Response, ContractError> {
        self.pool
            .update(storage, |mut state| -> Result<_, ContractError> {
                state.min_collateral_ratio = min_collateral_ratio;
                Ok(state)
            })
            .map(|_| {
                Response::new()
                    .add_attribute("action", "transfer_ownership")
                    .add_attribute("min_collateral_ratio", min_collateral_ratio)
            })
    }

    pub fn set_synth_address(
        &self,
        storage: &mut dyn Storage,
        addr: Addr,
    ) -> Result<Response, ContractError> {
        self.pool
            .update(storage, |mut x| -> Result<_, ContractError> {
                if x.synth != Addr::unchecked("") {
                    return Err(ContractError::SynthAlreadySet {});
                } else {
                    x.synth = addr.clone();
                    Ok(x)
                }
            })?;
        Ok(Response::new()
            .add_attribute("action", "set_synth_address")
            .add_attribute("synth", addr))
    }

    pub fn set_share_address(
        &self,
        storage: &mut dyn Storage,
        addr: Addr,
    ) -> Result<Response, ContractError> {
        self.pool
            .update(storage, |mut x| -> Result<_, ContractError> {
                if x.share != Addr::unchecked("") {
                    return Err(ContractError::ShareAlreadySet {});
                } else {
                    x.share = addr.clone();
                    Ok(x)
                }
            })?;
        Ok(Response::new()
            .add_attribute("action", "set_share_address")
            .add_attribute("share", addr))
    }

    fn get_pool(&self, storage: &dyn Storage) -> StdResult<PoolConfig> {
        self.pool.load(storage)
    }

    fn get_user(&self, storage: &dyn Storage, addr: &Addr) -> StdResult<UserInfo> {
        self.user
            .may_load(storage, addr)
            .map(|x| x.unwrap_or_default())
    }
}

pub const POOL: Pool = Pool::new();
