use cosmwasm_std::{Addr, Uint128};
use cw20::Cw20CoinVerified;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

// (de)serialize i128, see https://github.com/CosmWasm/cosmwasm/issues/1114
pub mod int128 {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bigint: &i128, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&bigint.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<i128, D::Error>
    where
        D: Deserializer<'de>,
    {
        let str = String::deserialize(deserializer)?;
        str::parse::<i128>(&str).map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BoostToken {
    pub addr: Addr,
    pub multiplier: Uint128,
    pub total_staked: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfo {
    pub want_token: Addr,
    pub reward_token: Addr,
    pub position_token: Addr,
    pub reward_per_second: Uint128,
    pub acc_reward_per_share: Uint128,
    pub last_update_timestamp: u64,
    pub boost_tokens: Vec<BoostToken>,
    pub total_staked: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserBoostAmount {
    pub addr: Addr,
    pub amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct UserInfo {
    pub amount: Uint128,
    #[serde(with = "int128")]
    #[schemars(with = "i128")]
    pub reward_debt: i128,
    pub boost_token: Vec<UserBoostAmount>,
}

impl UserInfo {
    pub fn add_boost_token(&mut self, token: Addr, amount: Uint128) {
        match self.boost_token.iter_mut().find(|x| x.addr == token) {
            Some(pivot) => pivot.amount += amount,
            None => self.boost_token.push(UserBoostAmount {
                addr: token,
                amount,
            }),
        }
    }
}

pub const ACC_REWARD_PRECISION: Uint128 = Uint128::new(10u128.pow(6));
pub const BOOST_MULTIPLIER_PRECISION: Uint128 = Uint128::new(10u128.pow(6));

impl PoolInfo {
    pub fn pending_reward(&self, user: &UserInfo, now: u64) -> Uint128 {
        let acc_reward_per_share = self.calculate_acc_reward_per_share(now);
        let user_weight = self.calc_user_weight(user);
        let reward =
            to_i128(user_weight * acc_reward_per_share / ACC_REWARD_PRECISION) - user.reward_debt;
        to_uint128(reward)
    }

    pub fn deposit(&mut self, user: &mut UserInfo, now: u64, amount: Uint128) {
        self.update_reward(now);
        user.amount += amount;
        user.reward_debt += to_i128(amount * self.acc_reward_per_share / ACC_REWARD_PRECISION);
        self.total_staked += amount;
    }

    pub fn deposit_boost_token(
        &mut self,
        user: &mut UserInfo,
        now: u64,
        token: Addr,
        amount: Uint128,
    ) {
        self.update_reward(now);
        match self.boost_tokens.iter_mut().find(|x| x.addr == token) {
            Some(entry) => {
                let boosted_amount = entry.multiplier * amount / BOOST_MULTIPLIER_PRECISION;
                user.reward_debt +=
                    to_i128(boosted_amount * self.acc_reward_per_share / ACC_REWARD_PRECISION);
                user.add_boost_token(token, amount);
                entry.total_staked += amount;
            }
            None => (),
        }
    }

    pub fn withdraw(
        &mut self,
        user: &mut UserInfo,
        now: u64,
        amount: Uint128,
    ) -> Vec<Cw20CoinVerified> {
        let mut send_tokens: Vec<Cw20CoinVerified> = vec![Cw20CoinVerified {
            address: self.want_token.clone(),
            amount,
        }];
        self.update_reward(now);
        self.total_staked -= amount;
        let user_weight_prior = self.calc_user_weight(user);

        user.amount -= amount;
        if user.amount.is_zero() {
            for UserBoostAmount { addr, amount } in &user.boost_token {
                send_tokens.push(Cw20CoinVerified {
                    address: addr.clone(),
                    amount: amount.clone(),
                });
                match self.boost_tokens.iter_mut().find(|x| &x.addr == addr) {
                    Some(entry) => {
                        entry.total_staked -= amount;
                    }
                    None => (),
                }
            }
            user.boost_token.clear();
        }

        let withdrawal_weight = user_weight_prior - self.calc_user_weight(user);
        user.reward_debt -=
            to_i128(withdrawal_weight * self.acc_reward_per_share / ACC_REWARD_PRECISION);

        send_tokens
    }

    pub fn harvest(&mut self, user: &mut UserInfo, now: u64) -> Uint128 {
        self.update_reward(now);
        let user_weight = self.calc_user_weight(user);
        let accumulated_reward =
            to_i128(self.acc_reward_per_share * user_weight / ACC_REWARD_PRECISION);
        let reward = accumulated_reward - user.reward_debt;
        user.reward_debt = accumulated_reward;
        to_uint128(reward)
    }

    pub fn withdraw_and_harvest(
        &mut self,
        user: &mut UserInfo,
        now: u64,
        amount: Uint128,
    ) -> Vec<Cw20CoinVerified> {
        let mut send_tokens: Vec<Cw20CoinVerified> = vec![Cw20CoinVerified {
            address: self.want_token.clone(),
            amount,
        }];

        self.update_reward(now);
        let user_weight = self.calc_user_weight(user);
        let accumulated_reward =
            to_i128(self.acc_reward_per_share * user_weight / ACC_REWARD_PRECISION);
        let reward_amount = to_uint128(accumulated_reward - user.reward_debt);

        send_tokens.push(Cw20CoinVerified {
            address: self.reward_token.clone(),
            amount: reward_amount,
        });

        user.reward_debt = accumulated_reward;
        user.amount -= amount;
        user.reward_debt =
            accumulated_reward - to_i128(amount * self.acc_reward_per_share / ACC_REWARD_PRECISION);
        self.total_staked -= amount;

        if user.amount.is_zero() {
            for UserBoostAmount { addr, amount } in &user.boost_token {
                send_tokens.push(Cw20CoinVerified {
                    address: addr.clone(),
                    amount: amount.clone(),
                });
                match self.boost_tokens.iter_mut().find(|x| &x.addr == addr) {
                    Some(entry) => {
                        entry.total_staked -= amount;
                    }
                    None => (),
                }
            }
            user.boost_token.clear();
        }

        send_tokens
    }

    pub fn set_reward_per_second(&mut self, now: u64, reward_per_second: Uint128) {
        self.update_reward(now);
        self.reward_per_second = reward_per_second;
    }

    pub fn set_boost_token(&mut self, token: Addr, multiplier: Uint128, now: u64) {
        self.update_reward(now);
        match self.boost_tokens.iter_mut().find(|x| x.addr == token) {
            Some(pivot) => pivot.multiplier = multiplier,
            None => self.boost_tokens.push(BoostToken {
                addr: token,
                multiplier,
                total_staked: Uint128::zero(),
            }),
        }
    }

    fn update_reward(&mut self, now: u64) {
        self.acc_reward_per_share = self.calculate_acc_reward_per_share(now);
        self.last_update_timestamp = now;
    }

    fn calculate_acc_reward_per_share(&self, now: u64) -> Uint128 {
        let total_weight = self.calc_total_weight();
        if now > self.last_update_timestamp && !total_weight.is_zero() {
            let elapsed = now - self.last_update_timestamp;
            let reward_amount = self.reward_per_second * Uint128::from(elapsed);
            return self.acc_reward_per_share
                + (reward_amount * ACC_REWARD_PRECISION / total_weight);
        } else {
            return self.acc_reward_per_share;
        }
    }

    fn calc_user_weight(&self, user: &UserInfo) -> Uint128 {
        let mut weight = user.amount;
        for BoostToken {
            addr: token_addr,
            multiplier,
            ..
        } in &self.boost_tokens
        {
            match user.boost_token.iter().find(|&x| &x.addr == token_addr) {
                Some(pivot) => weight += pivot.amount * multiplier / BOOST_MULTIPLIER_PRECISION,
                None => {}
            }
        }

        weight
    }

    pub fn calc_total_weight(&self) -> Uint128 {
        let mut weight = self.total_staked * BOOST_MULTIPLIER_PRECISION;
        for BoostToken {
            multiplier,
            total_staked,
            ..
        } in &self.boost_tokens
        {
            weight += total_staked * multiplier
        }
        return weight / BOOST_MULTIPLIER_PRECISION;
    }
}

fn to_i128(n: Uint128) -> i128 {
    n.u128().try_into().unwrap()
}

fn to_uint128(n: i128) -> Uint128 {
    n.try_into().map(|x| Uint128::new(x)).unwrap()
}
