use crate::{
    helpers::{SwapPairUtils, Unit},
    pool::PoolConfig,
};
use cosmwasm_std::{Addr, Env, QuerierWrapper, Response, StdError, StdResult, Storage, Uint128};
use cw20::{Cw20QueryMsg, TokenInfoResponse};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Epoch not elapsed")]
    EpochNotElapsed {},

    #[error("Mint amount exceed allowance")]
    MintAmountTooLarge {},

    #[error("Invalid config: {msg}")]
    InvalidConfig { msg: String },
}

/// the mintable amount of the next epoch is calculated base on the TWAP
/// in prior one
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pair_addr: Addr,
    base_index: u8,
    price_cumulative_last: Uint128,
    start_timestamp: u64,
    epoch_duration: u64,
    base_supply: Uint128,
    max_supply: Option<Uint128>,
    ceil_price: Option<Uint128>,
    max_expansion_rate: Option<Uint128>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            epoch_duration: 0,
            pair_addr: Addr::unchecked(""),
            base_index: 0,
            price_cumulative_last: Uint128::zero(),
            start_timestamp: 0,
            base_supply: Uint128::zero(),
            max_supply: None,
            ceil_price: None,
            max_expansion_rate: None,
        }
    }
}

const SUPPLY_TIERS: [Uint128; 9] = [
    Unit::luna(0),
    Unit::luna(500_000),
    Unit::luna(1_000_000),
    Unit::luna(1_500_000),
    Unit::luna(2_000_000),
    Unit::luna(5_000_000),
    Unit::luna(10_000_000),
    Unit::luna(20_000_000),
    Unit::luna(50_000_000),
];

const EXPANSION_RATES: [u128; 9] = [450, 400, 350, 300, 250, 200, 150, 125, 100];

impl State {
    fn get_expansion_rate(&self, supply: Uint128, twap: Uint128) -> Option<Uint128> {
        if self.ceil_price.is_none() {
            None
        } else if twap <= self.ceil_price.unwrap() {
            None
        } else {
            let tier = SUPPLY_TIERS.iter().position(|&x| supply < x)?;
            let &rate = EXPANSION_RATES.get(tier - 1)?;

            if let Some(max) = self.max_expansion_rate {
                Some(Uint128::from(rate).max(max))
            } else {
                Some(Uint128::from(rate))
            }
        }
    }

    fn next(&mut self, token_supply: Uint128, price_cumulative: Uint128, now: u64) -> Uint128 {
        if self.start_timestamp == 0 {
            // first epoch
            self.start_timestamp = now;
            self.price_cumulative_last = price_cumulative;
            return Uint128::zero();
        }

        let twap = (price_cumulative - self.price_cumulative_last)
            / Uint128::from(now - self.start_timestamp);

        self.start_timestamp = now;
        self.price_cumulative_last = price_cumulative;
        self.base_supply = token_supply;
        self.max_supply = self
            .get_expansion_rate(token_supply, twap)
            .or(Some(Uint128::zero()))
            .map(|x| token_supply.multiply_ratio(x + Unit::precision(), Unit::precision()));
        twap
    }

    fn get_allowed_supply(&self, now: u64) -> Option<Uint128> {
        match self.max_supply {
            None => None,
            Some(max_supply) => {
                let elapsed = now - self.start_timestamp;
                let fully_expaded = self.epoch_duration / 2;
                if elapsed > fully_expaded {
                    Some(max_supply)
                } else {
                    Some(
                        self.base_supply
                            + (max_supply - self.base_supply)
                                .multiply_ratio(elapsed, fully_expaded),
                    )
                }
            }
        }
    }
}

pub struct Epoch<'a>(Item<'a, State>);

const NAMESPACE: &str = "EPOCH";

impl<'a> Epoch<'a> {
    pub const fn new() -> Self {
        Self(Item::new(NAMESPACE))
    }

    pub fn initialize(&self, storage: &mut dyn Storage) -> StdResult<()> {
        self.0.save(storage, &State::default())?;
        Ok(())
    }

    pub fn assert_mint_amount(
        &self,
        storage: &dyn Storage,
        querier: &QuerierWrapper,
        pool: &PoolConfig,
        mint_amount: Uint128,
        now: u64,
    ) -> Result<(), Error> {
        let state = self.get(storage)?;
        let current_supply = Epoch::get_token_supply(querier, &pool.synth)?;

        match state.get_allowed_supply(now) {
            None => Ok(()),
            Some(max_supply) => {
                if current_supply + mint_amount > max_supply {
                    Err(Error::MintAmountTooLarge {})
                } else {
                    Ok(())
                }
            }
        }
    }

    pub fn next_epoch(
        &self,
        storage: &mut dyn Storage,
        querier: &QuerierWrapper,
        env: Env,
        pool: &PoolConfig,
    ) -> Result<Response, Error> {
        let mut state = self.get(storage)?;
        let now = env.block.time.seconds();

        if state.start_timestamp + state.epoch_duration > now {
            return Err(Error::EpochNotElapsed {});
        }

        let price_cumulative =
            SwapPairUtils::query_cumulative_prices(querier, &state.pair_addr, state.base_index)?;
        let token_supply = Epoch::get_token_supply(querier, &pool.synth)?;

        let twap = state.next(token_supply, price_cumulative, now);
        self.0.save(storage, &state)?;

        Ok(Response::new()
            .add_attribute("action", "update_epoch")
            .add_attribute("twap", twap))
    }

    pub fn config_oracle(
        &self,
        storage: &mut dyn Storage,
        querier: &QuerierWrapper,
        pair_addr: &Addr,
        base_index: u8,
        now: u64,
    ) -> Result<(), Error> {
        if base_index > 1 {
            return Err(Error::Std(StdError::generic_err(
                "Token index should be 0 or 1",
            )));
        }
        let cumulative_price =
            SwapPairUtils::query_cumulative_prices(querier, &pair_addr, base_index)?;

        self.0.update(storage, |mut state| -> Result<_, StdError> {
            state.pair_addr = pair_addr.clone();
            state.base_index = base_index;
            state.start_timestamp = now;
            state.price_cumulative_last = cumulative_price;
            Ok(state)
        })?;
        Ok(())
    }

    pub fn config_epoch(
        &self,
        storage: &mut dyn Storage,
        epoch_duration: u64,
        ceil_price: Option<Uint128>,
        max_expansion_rate: Option<Uint128>,
    ) -> Result<Response, Error> {
        if ceil_price.is_some() && ceil_price.unwrap() < Unit::precision() {
            return Err(Error::InvalidConfig {
                msg: String::from("Ceil price cannot be lower than 1"),
            });
        }

        self.0.update(storage, |mut state| -> Result<_, StdError> {
            state.ceil_price = ceil_price;
            state.max_expansion_rate = max_expansion_rate;
            state.epoch_duration = epoch_duration;
            Ok(state)
        })?;

        Ok(Response::new()
            .add_attribute("action", "config_epoch")
            .add_attribute("epoch_duration", epoch_duration.to_string()))
    }

    fn get_token_supply(querier: &QuerierWrapper, token: &Addr) -> Result<Uint128, StdError> {
        let TokenInfoResponse { total_supply, .. } =
            querier.query_wasm_smart(token, &Cw20QueryMsg::TokenInfo {})?;
        Ok(total_supply)
    }

    fn get(&self, storage: &dyn Storage) -> StdResult<State> {
        self.0.load(storage)
    }
}

pub const EPOCH: Epoch = Epoch::new();
