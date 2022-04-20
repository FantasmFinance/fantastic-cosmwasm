use astroport::pair::CumulativePricesResponse;
use cosmwasm_std::{Addr, QuerierWrapper, StdError, Uint128};

pub struct SwapPairUtils;

impl SwapPairUtils {
    pub fn query_cumulative_prices(
        querier: &QuerierWrapper,
        pair_addr: &Addr,
        base_index: u8,
    ) -> Result<Uint128, StdError> {
        let response: CumulativePricesResponse =
            querier.query_wasm_smart(pair_addr, &astroport::pair::QueryMsg::CumulativePrices {})?;

        let cumulative_price = if base_index == 0 {
            response.price0_cumulative_last
        } else {
            response.price1_cumulative_last
        };

        Ok(cumulative_price)
    }
}

pub struct Unit;

impl Unit {
    pub const fn luna(value: u128) -> Uint128 {
        Uint128::new(value * 10u128.pow(6))
    }

    pub const fn precision() -> Uint128 {
        Uint128::new(1_000_000u128)
    }
}
