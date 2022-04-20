use astroport::pair::PoolResponse;
use cosmwasm_std::{Addr, QuerierWrapper, StdError, StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    helpers::{SwapPairUtils, Unit},
    ContractError,
};

pub const SYNTH_ORACLE: PairOracle = PairOracle::new("synth_oracle");

pub const SHARE_ORACLE: PairOracle = PairOracle::new("share_oracle");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PairOracleState {
    pub pair_addr: Addr,
    pub base_index: u8,
    pub price_cumulative_last: Uint128,
    pub twap: Option<Uint128>,
    pub last_update: u64,
    pub twap_period: u64,
}

impl Default for PairOracleState {
    fn default() -> Self {
        Self {
            pair_addr: Addr::unchecked(""),
            base_index: 0,
            price_cumulative_last: Uint128::zero(),
            twap: None,
            last_update: 0,
            twap_period: 600,
        }
    }
}

impl PairOracleState {
    pub fn get_spot_price(&self, querier: &QuerierWrapper) -> Result<Uint128, ContractError> {
        let (base_reserve, quote_reserve) =
            PairOracleState::get_pair_reserve(querier, &self.pair_addr, self.base_index.into())?;

        Ok(quote_reserve * Unit::precision() / base_reserve)
    }

    fn get_pair_reserve(
        querier: &QuerierWrapper,
        pair_addr: &Addr,
        base_index: usize,
    ) -> Result<(Uint128, Uint128), ContractError> {
        let pair_info: PoolResponse =
            querier.query_wasm_smart(pair_addr, &astroport::pair::QueryMsg::Pool {})?;

        let quote_index = base_index + 1 % 2; // since assets length alway be 2
        let base_reserve = pair_info
            .assets
            .get(base_index)
            .ok_or(ContractError::Std(StdError::generic_err(
                "invalid pool info",
            )))?
            .amount;
        let quote_reserve = pair_info
            .assets
            .get(quote_index)
            .ok_or(ContractError::Std(StdError::generic_err(
                "invalid pool info",
            )))?
            .amount;
        Ok((base_reserve, quote_reserve))
    }
}

pub struct PairOracle<'a>(Item<'a, PairOracleState>);

impl<'a> PairOracle<'a> {
    pub const fn new(storage_key: &'a str) -> Self {
        PairOracle(Item::new(storage_key))
    }

    pub fn initialize(&self, storage: &mut dyn Storage) -> StdResult<()> {
        self.0.save(storage, &PairOracleState::default())
    }

    pub fn get_twap(&self, storage: &dyn Storage) -> Result<(Uint128, u64), ContractError> {
        let PairOracleState {
            twap, last_update, ..
        } = self.0.load(storage)?;
        if twap.is_none() {
            return Err(ContractError::PriceUnavailableOrOutdated {});
        }

        Ok((twap.unwrap(), last_update))
    }

    pub fn get_state(&self, storage: &dyn Storage) -> Result<PairOracleState, ContractError> {
        let state = self.0.load(storage)?;
        Ok(state)
    }

    pub fn get_spot_price(
        &self,
        storage: &dyn Storage,
        querier: &QuerierWrapper,
    ) -> Result<Uint128, ContractError> {
        let state = self.0.load(storage)?;
        state.get_spot_price(querier)
    }

    /// recalculate TWAP when period elapsed
    pub fn update_twap(
        &self,
        storage: &mut dyn Storage,
        querier: &QuerierWrapper,
        now: u64,
    ) -> Result<PairOracleState, ContractError> {
        let mut state = self.0.load(storage)?;
        let next_update = state.last_update + state.twap_period;
        if next_update > now {
            return Err(ContractError::TwapPeriodNotElapsed {
                time: Timestamp::from_seconds(next_update),
            });
        }
        let cumulative_price =
            SwapPairUtils::query_cumulative_prices(&querier, &state.pair_addr, state.base_index)?;
        state.twap = Some(
            (cumulative_price - state.price_cumulative_last)
                / Uint128::from(now - state.last_update),
        );
        state.price_cumulative_last = cumulative_price;
        state.last_update = now;

        self.0.save(storage, &state)?;
        Ok(state)
    }

    /// set oracle config. Allow from admin only
    pub fn config(
        &self,
        storage: &mut dyn Storage,
        querier: &QuerierWrapper,
        pair_addr: &Addr,
        base_index: u8,
        twap_period: u64,
        now: u64,
    ) -> Result<(), ContractError> {
        if base_index > 1 {
            return Err(ContractError::Std(StdError::generic_err(
                "Token index should be 0 or 1",
            )));
        }
        let oracle = self.0.may_load(storage)?;

        let cumulative_price =
            SwapPairUtils::query_cumulative_prices(querier, &pair_addr, base_index)?;
        let state = if oracle.is_some() {
            let mut oracle = oracle.unwrap();
            oracle.pair_addr = pair_addr.clone();
            oracle.base_index = base_index;
            oracle.last_update = now;
            oracle.price_cumulative_last = cumulative_price;
            oracle.twap = None;
            oracle.twap_period = twap_period;
            oracle
        } else {
            PairOracleState {
                pair_addr: pair_addr.clone(),
                base_index,
                last_update: now,
                price_cumulative_last: cumulative_price,
                twap: None,
                twap_period,
            }
        };

        self.0.save(storage, &state)?;

        Ok(())
    }
}
