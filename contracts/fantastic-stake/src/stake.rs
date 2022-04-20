use cosmwasm_std::{
    to_binary, Addr, Deps, DepsMut, Env, Response, StdResult, Storage, Uint128, WasmMsg,
};
use cw20::{Cw20CoinVerified, Cw20ExecuteMsg};
use cw_storage_plus::{Item, Map};

use crate::{
    msg::UserInfoResponse,
    pool::{PoolInfo, UserInfo},
    ContractError,
};

pub struct Stake<'a> {
    user: Map<'a, &'a Addr, UserInfo>,
    pool: Item<'a, PoolInfo>,
}

impl<'a> Stake<'a> {
    pub const fn new() -> Self {
        Self {
            user: Map::new("user"),
            pool: Item::new("pool"),
        }
    }

    pub fn initialize(
        &self,
        storage: &mut dyn Storage,
        now: u64,
        want_token: Addr,
        reward_token: Addr,
    ) -> StdResult<()> {
        let pool = PoolInfo {
            acc_reward_per_share: Uint128::zero(),
            want_token,
            reward_token,
            position_token: Addr::unchecked(""),
            reward_per_second: Uint128::zero(),
            last_update_timestamp: now,
            boost_tokens: vec![],
            total_staked: Uint128::zero(),
        };

        self.pool.save(storage, &pool)
    }

    pub fn deposit(
        &self,
        deps: DepsMut,
        env: Env,
        sender: &Addr,
        coin: Cw20CoinVerified,
    ) -> Result<Response, ContractError> {
        let mut pool = self.get_pool(deps.storage)?;
        let mut user = self.get_user_info(deps.storage, sender)?;

        if pool.want_token != coin.address {
            return Err(ContractError::InvalidDepositToken {
                want: pool.want_token,
                sent: coin.address.clone(),
            });
        }

        pool.deposit(&mut user, env.block.time.seconds(), coin.amount);

        self.user.save(deps.storage, sender, &user)?;
        self.pool.save(deps.storage, &pool)?;

        let messages: Vec<WasmMsg> = vec![WasmMsg::Execute {
            contract_addr: pool.position_token.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Mint {
                recipient: sender.to_string(),
                amount: coin.amount,
            })?,
            funds: vec![],
        }];

        Ok(Response::new()
            .add_attribute("action", "deposit")
            .add_attribute("amount", coin.amount)
            .add_messages(messages))
    }

    pub fn deposit_boost_token(
        &self,
        deps: DepsMut,
        env: Env,
        sender: &Addr,
        coin_amount: Cw20CoinVerified,
    ) -> Result<Response, ContractError> {
        let mut pool = self.get_pool(deps.storage)?;
        let mut user = self.get_user_info(deps.storage, sender)?;
        if user.amount.is_zero() {
            return Err(ContractError::BoostWithEmptyDeposit {});
        }
        pool.deposit_boost_token(
            &mut user,
            env.block.time.seconds(),
            coin_amount.address,
            coin_amount.amount,
        );

        self.user.save(deps.storage, sender, &user)?;
        self.pool.save(deps.storage, &pool)?;

        Ok(Response::new().add_attribute("action", "deposit_boost_token"))
    }

    pub fn withdraw(
        &self,
        deps: DepsMut,
        env: Env,
        sender: &Addr,
        coin: Cw20CoinVerified,
    ) -> Result<Response, ContractError> {
        let mut user = self.get_user_info(deps.storage, sender)?;
        let mut pool = self.get_pool(deps.storage)?;

        if pool.position_token != coin.address {
            return Err(ContractError::WithdrawInvalidPositionToken);
        }

        if user.amount < coin.amount {
            return Err(ContractError::WithdrawTooMuch);
        }

        if coin.amount.is_zero() {
            return Err(ContractError::WithdrawInvalidPositionToken);
        }

        let send_tokens = pool.withdraw(&mut user, env.block.time.seconds(), coin.amount);

        self.pool.save(deps.storage, &pool)?;
        self.user.save(deps.storage, sender, &user)?;

        let mut messages: Vec<WasmMsg> = vec![WasmMsg::Execute {
            contract_addr: pool.position_token.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Burn {
                amount: coin.amount,
            })?,
            funds: vec![],
        }];
        messages.extend(send_tokens.iter().map(|coin| {
            WasmMsg::Execute {
                contract_addr: coin.address.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: sender.to_string(),
                    amount: coin.amount,
                })
                .unwrap(),
                funds: vec![],
            }
        }));

        Ok(Response::new()
            .add_attribute("action", "withdraw")
            .add_attribute("amount", coin.amount)
            .add_messages(messages))
    }

    pub fn withdraw_and_harvest(
        &self,
        deps: DepsMut,
        env: Env,
        sender: &Addr,
        coin: Cw20CoinVerified,
    ) -> Result<Response, ContractError> {
        let mut user = self.get_user_info(deps.storage, sender)?;
        let mut pool = self.get_pool(deps.storage)?;

        if pool.position_token != coin.address {
            return Err(ContractError::WithdrawInvalidPositionToken);
        }

        if user.amount < coin.amount {
            return Err(ContractError::WithdrawTooMuch);
        }

        if coin.amount.is_zero() {
            return Err(ContractError::WithdrawInvalidPositionToken);
        }

        let send_tokens =
            pool.withdraw_and_harvest(&mut user, env.block.time.seconds(), coin.amount);

        self.pool.save(deps.storage, &pool)?;
        self.user.save(deps.storage, sender, &user)?;

        let mut messages: Vec<WasmMsg> = vec![WasmMsg::Execute {
            contract_addr: pool.position_token.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Burn {
                amount: coin.amount,
            })?,
            funds: vec![],
        }];
        messages.extend(send_tokens.iter().map(|coin| {
            WasmMsg::Execute {
                contract_addr: coin.address.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: sender.to_string(),
                    amount: coin.amount,
                })
                .unwrap(),
                funds: vec![],
            }
        }));

        Ok(Response::new()
            .add_attribute("action", "withdraw_and_harvest")
            .add_attribute("amount", coin.amount)
            .add_messages(messages))
    }

    pub fn harvest(
        &self,
        deps: DepsMut,
        env: Env,
        sender: &Addr,
    ) -> Result<Response, ContractError> {
        let mut user = self.get_user_info(deps.storage, sender)?;
        let mut pool = self.get_pool(deps.storage)?;

        let reward_amount = pool.harvest(&mut user, env.block.time.seconds());

        self.pool.save(deps.storage, &pool)?;
        self.user.save(deps.storage, sender, &user)?;

        let transfer_msg = WasmMsg::Execute {
            contract_addr: pool.reward_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: sender.to_string(),
                amount: reward_amount,
            })?,
            funds: vec![],
        };

        Ok(Response::new()
            .add_attribute("action", "harvest")
            .add_message(transfer_msg))
    }

    pub fn set_reward_per_second(
        &self,
        deps: DepsMut,
        env: Env,
        reward_per_second: Uint128,
    ) -> Result<Response, ContractError> {
        let mut pool = self.get_pool(deps.storage)?;
        pool.set_reward_per_second(env.block.time.seconds(), reward_per_second);
        self.pool.save(deps.storage, &pool)?;
        Ok(Response::new()
            .add_attribute("action", "set_reward_per_block")
            .add_attribute("reward_per_block", reward_per_second))
    }

    pub fn set_boost_token(
        &self,
        deps: DepsMut,
        env: Env,
        token: Addr,
        multiplier: Uint128,
    ) -> Result<Response, ContractError> {
        let mut pool = self.get_pool(deps.storage)?;
        pool.set_boost_token(token.clone(), multiplier, env.block.time.seconds());
        self.pool.save(deps.storage, &pool)?;
        Ok(Response::new()
            .add_attribute("action", "set_boost_token")
            .add_attribute("token", token)
            .add_attribute("multiplier", multiplier))
    }

    pub fn pending_reward(&self, deps: Deps, env: Env, user: &Addr) -> StdResult<Uint128> {
        let user_info = self.get_user_info(deps.storage, user)?;
        let pool = self.get_pool(deps.storage)?;
        Ok(pool.pending_reward(&user_info, env.block.time.seconds()))
    }

    pub fn get_pool(&self, storage: &dyn Storage) -> StdResult<PoolInfo> {
        self.pool.load(storage)
    }

    pub fn get_user_info(&self, storage: &dyn Storage, addr: &Addr) -> StdResult<UserInfo> {
        let user = self.user.may_load(storage, addr)?.unwrap_or_default();
        Ok(user)
    }

    pub fn query_user_info(
        &self,
        storage: &dyn Storage,
        addr: &Addr,
        now: u64,
    ) -> StdResult<UserInfoResponse> {
        let user = self.user.may_load(storage, addr)?.unwrap_or_default();
        let pool = self.get_pool(storage)?;
        Ok(UserInfoResponse {
            amount: user.amount,
            reward_debt: user.reward_debt.to_string(),
            boost_token: user.boost_token.clone(),
            pending_reward: pool.pending_reward(&user, now),
        })
    }

    pub fn set_position_token(
        &self,
        storage: &mut dyn Storage,
        addr: &Addr,
    ) -> Result<Response, ContractError> {
        self.pool
            .update(storage, |mut x| -> Result<_, ContractError> {
                if x.position_token != Addr::unchecked("") {
                    // already set
                    return Err(ContractError::Unauthorized {});
                }
                x.position_token = addr.clone();
                Ok(x)
            })?;
        Ok(Response::new()
            .add_attribute("action", "set_position_token")
            .add_attribute("position_token", addr))
    }
}

pub const STAKE: Stake = Stake::new();
