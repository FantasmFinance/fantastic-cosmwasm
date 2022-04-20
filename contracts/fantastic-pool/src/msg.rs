use crate::oracle::PairOracleState;
use cosmwasm_std::{Addr, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub collateral_denom: String,
    pub token_code_id: u64,
    pub synth_symbol: String,
    pub synth_name: String,
    pub share_symbol: String,
    pub share_name: String,
    pub share_max_cap: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Mint {
        min_synth_out: Uint128,
    },
    Receive(Cw20ReceiveMsg),
    Collect {},
    UpdateOracle {},
    SetFee {
        minting_fee: Uint128,
        redemption_fee: Uint128,
    },
    TransferOwnership {
        new_owner: Addr,
    },
    AcceptOwnership {},
    Toggle {
        mint_paused: bool,
        redeem_paused: bool,
    },
    SetMinCollateralRatio {
        value: Uint128,
    },
    ConfigShareOracle {
        pair_addr: Addr,
        base_index: u8,
        twap_period: u64,
    },
    ConfigSynthOracle {
        pair_addr: Addr,
        base_index: u8,
        twap_period: u64,
    },
    RefreshCollateralRatio {},
    UpdateEpoch {},
    SetEpochConfig {
        ceil_price: Option<Uint128>,
        epoch_duration: u64,
        max_expansion_rate: Option<Uint128>,
    },

    /// internal use only
    BurnShare {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20CallbackMsg {
    Redeem {
        min_collateral_out: Uint128,
        min_share_out: Uint128,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// get pool config and balance
    GetPoolInfo {},
    /// get info of particular user
    GetUserInfo {
        address: Addr,
    },
    CalcMint {
        collateral_amount: Uint128,
    },
    CalcRedeem {
        synth_amount: Uint128,
    },
    GetPrice {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OracleInfoResponse {
    pub synth: PairOracleState,
    pub share: PairOracleState,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfoResponse {
    pub collateral_denom: String,
    pub synth: Addr,
    pub share: Addr,
    pub collateral_ratio: Uint128,
    pub min_collateral_ratio: Uint128,
    pub last_refresh_collateral_ratio: u64,
    pub price_band: Uint128,
    pub collateral_ratio_step: Uint128,
    pub refresh_collateral_ratio_cooldown: u64,
    pub collateral_balance: Uint128,
    pub minting_fee: Uint128,
    pub redemption_fee: Uint128,
    pub total_unclaimed_collateral: Uint128,
    pub total_unclaimed_synth: Uint128,
    pub total_unclaimed_share: Uint128,
    pub oracle: OracleInfoResponse,
    pub mint_paused: bool,
    pub redeem_paused: bool,
    pub owner: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CalcMintResult {
    pub synth_out: Uint128,
    pub buy_share_value: Uint128,
    pub fee: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CalcRedeemResult {
    pub collateral_out: Uint128,
    pub share_out: Uint128,
    pub fee: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetPriceResult {
    pub share_spot: Uint128,
    pub synth_spot: Uint128,
    pub share_twap: Option<Uint128>,
    pub synth_twap: Option<Uint128>,
}
