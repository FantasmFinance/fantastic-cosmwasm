use cosmwasm_std::{Addr, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::pool::UserBoostAmount;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub want_token: Addr,
    pub reward_token: Addr,
    /// position token
    pub token_symbol: String,
    pub token_name: String,
    pub token_code_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    Harvest { to: Option<Addr> },
    SetRewardPerSecond { reward_per_second: Uint128 },
    SetBoostToken { address: Addr, multiplier: Uint128 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20ReceiveCallbackMsg {
    Deposit { to: Option<Addr> },
    DepositBoostToken { to: Option<Addr> },
    Withdraw {},
    WithdrawAndHarvest {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    PoolInfo {},
    UserInfo { user: Addr },
    PendingReward { user: Addr },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct UserInfoResponse {
    pub amount: Uint128,
    pub reward_debt: String,
    pub boost_token: Vec<UserBoostAmount>,
    pub pending_reward: Uint128,
}
