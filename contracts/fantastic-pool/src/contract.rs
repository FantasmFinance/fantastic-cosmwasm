use crate::epoch::EPOCH;
use crate::error::ContractError;
use crate::msg::{
    CalcMintResult, CalcRedeemResult, Cw20CallbackMsg, ExecuteMsg, GetPriceResult, InstantiateMsg,
    MigrateMsg, OracleInfoResponse, PoolInfoResponse, QueryMsg,
};
use crate::oracle::{SHARE_ORACLE, SYNTH_ORACLE};
use crate::pool::{UserInfo, POOL};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response,
    StdError, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{
    BalanceResponse, Cw20Coin, Cw20CoinVerified, Cw20ExecuteMsg, Cw20QueryMsg, Cw20ReceiveMsg,
    MinterResponse,
};
use cw_utils::parse_reply_instantiate_data;
use ownable::OWNABLE;
use std::{env, vec};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:fantastic_pool";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const INSTANTIATE_SYNTH_TOKEN_REPLY_ID: u64 = 1;
const INSTANTIATE_SHARE_TOKEN_REPLY_ID: u64 = 2;

// ====== CONSTRUCTOR ======
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    if msg.share_max_cap.is_zero() {
        return Err(ContractError::Std(StdError::generic_err(
            "Share max cap must be greater than zero",
        )));
    }
    OWNABLE.initialize(deps.storage, info.sender.clone())?;
    POOL.initialize(deps.storage, msg.collateral_denom.clone())?;
    EPOCH.initialize(deps.storage)?;
    SYNTH_ORACLE.initialize(deps.storage)?;
    SHARE_ORACLE.initialize(deps.storage)?;

    let initial_synth_balances = info
        .funds
        .iter()
        .filter(|&x| x.denom == msg.collateral_denom)
        .map(|x| x.amount)
        .map(|x| Cw20Coin {
            address: info.sender.to_string(),
            amount: x,
        })
        .collect();

    let messages = vec![
        SubMsg::reply_on_success(
            WasmMsg::Instantiate {
                admin: None,
                code_id: msg.token_code_id,
                msg: to_binary(&cw20_base::msg::InstantiateMsg {
                    decimals: 6,
                    mint: Some(MinterResponse {
                        cap: None,
                        minter: env.contract.address.to_string(),
                    }),
                    initial_balances: initial_synth_balances,
                    name: msg.synth_name,
                    symbol: msg.synth_symbol,
                    marketing: None,
                })?,
                funds: vec![],
                label: String::from("Instantiate fantastic synth token"),
            },
            INSTANTIATE_SYNTH_TOKEN_REPLY_ID,
        ),
        SubMsg::reply_on_success(
            WasmMsg::Instantiate {
                admin: None,
                code_id: msg.token_code_id,
                msg: to_binary(&cw20_base::msg::InstantiateMsg {
                    decimals: 6,
                    mint: Some(MinterResponse {
                        cap: None,
                        minter: env.contract.address.to_string(),
                    }),
                    initial_balances: vec![Cw20Coin {
                        address: info.sender.to_string(),
                        amount: msg.share_max_cap,
                    }],
                    name: msg.share_name,
                    symbol: msg.share_symbol,
                    marketing: None,
                })?,
                funds: vec![],
                label: String::from("Instantiate fantastic share token"),
            },
            INSTANTIATE_SHARE_TOKEN_REPLY_ID,
        ),
    ];

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("owner", info.sender)
        .add_attribute("collateral_denom", msg.collateral_denom)
        .add_submessages(messages))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INSTANTIATE_SYNTH_TOKEN_REPLY_ID => {
            let res = parse_reply_instantiate_data(msg)?;
            let contract_addr = deps.api.addr_validate(&res.contract_address)?;
            POOL.set_synth_address(deps.storage, contract_addr)
        }
        INSTANTIATE_SHARE_TOKEN_REPLY_ID => {
            let res = parse_reply_instantiate_data(msg)?;
            let contract_addr = deps.api.addr_validate(&res.contract_address)?;
            POOL.set_share_address(deps.storage, contract_addr)
        }
        _ => Err(ContractError::Std(StdError::generic_err(
            "Invalid reply ID",
        ))),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}

// ====== MUTATION ======
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Mint { min_synth_out } => POOL.mint(
            deps.storage,
            &deps.querier,
            env,
            &info.sender,
            info.funds,
            min_synth_out,
        ),
        ExecuteMsg::Collect {} => POOL.collect(deps.storage, env, &info.sender),
        ExecuteMsg::RefreshCollateralRatio {} => POOL.refresh_collateral_ratio(deps.storage, env),
        ExecuteMsg::UpdateOracle {} => execute_update_oracle(deps, env.block.time.seconds()),
        ExecuteMsg::SetFee {
            minting_fee,
            redemption_fee,
        } => {
            OWNABLE.assert_owner(deps.storage, &info.sender)?;
            POOL.set_fee(deps.storage, minting_fee, redemption_fee)
        }
        ExecuteMsg::TransferOwnership { new_owner } => OWNABLE
            .execute_transfer_ownership(deps.storage, info, new_owner)
            .map_err(|e| ContractError::Ownable(e)),
        ExecuteMsg::AcceptOwnership {} => OWNABLE
            .execute_accept_ownership(deps.storage, info)
            .map_err(|e| ContractError::Ownable(e)),
        ExecuteMsg::Toggle {
            mint_paused,
            redeem_paused,
        } => {
            OWNABLE.assert_owner(deps.storage, &info.sender)?;
            POOL.toggle(deps.storage, mint_paused, redeem_paused)
        }
        ExecuteMsg::SetMinCollateralRatio { value } => {
            OWNABLE.assert_owner(deps.storage, &info.sender)?;
            POOL.set_min_collateral_ratio(deps.storage, value)
        }
        ExecuteMsg::ConfigShareOracle {
            pair_addr,
            base_index,
            twap_period,
        } => execute_config_share_oracle(
            deps,
            info.sender,
            pair_addr,
            base_index,
            twap_period,
            env.block.time.seconds(),
        ),
        ExecuteMsg::ConfigSynthOracle {
            pair_addr,
            base_index,
            twap_period,
        } => execute_config_synth_oracle(
            deps,
            info.sender,
            pair_addr,
            base_index,
            twap_period,
            env.block.time.seconds(),
        ),
        ExecuteMsg::UpdateEpoch {} => execute_update_epoch(deps, env),
        ExecuteMsg::BurnShare {} => execute_burn_share(deps, env),
        ExecuteMsg::Receive(cw20_msg) => execute_receive(deps, env, info.sender, cw20_msg),
        ExecuteMsg::SetEpochConfig {
            ceil_price,
            epoch_duration,
            max_expansion_rate,
        } => {
            OWNABLE.assert_owner(deps.storage, &info.sender)?;
            EPOCH
                .config_epoch(deps.storage, epoch_duration, ceil_price, max_expansion_rate)
                .map_err(|e| e.into())
        }
    }
}

fn execute_receive(
    deps: DepsMut,
    env: Env,
    token: Addr,
    envelop: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_binary(&envelop.msg) {
        Ok(Cw20CallbackMsg::Redeem {
            min_collateral_out,
            min_share_out,
        }) => {
            let synth_input = Cw20CoinVerified {
                address: token,
                amount: envelop.amount,
            };

            let sender = deps.api.addr_validate(&envelop.sender)?;
            POOL.redeem(
                deps.storage,
                &deps.querier,
                env,
                &sender,
                synth_input,
                min_collateral_out,
                min_share_out,
            )
        }
        Err(err) => {
            return Err(ContractError::Std(err));
        }
    }
}

pub fn execute_burn_share(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let pool = POOL.pool.load(deps.storage)?;
    let BalanceResponse {
        balance: burn_amount,
    } = deps.querier.query_wasm_smart(
        pool.share.to_string(),
        &Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        },
    )?;

    let msg = WasmMsg::Execute {
        contract_addr: pool.share.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Burn {
            amount: burn_amount,
        })?,
        funds: vec![],
    };

    Ok(Response::new()
        .add_attribute("action", "burn_share")
        .add_attribute("burn_amount", burn_amount)
        .add_message(msg))
}

fn execute_update_oracle(deps: DepsMut, now: u64) -> Result<Response, ContractError> {
    SHARE_ORACLE
        .update_twap(deps.storage, &deps.querier, now)
        .ok(); // ignore error
    SYNTH_ORACLE
        .update_twap(deps.storage, &deps.querier, now)
        .ok();

    Ok(Response::new()
        .add_attribute("action", "update_oracle")
        .add_attribute("timestamp", now.to_string()))
}

pub fn execute_config_share_oracle(
    deps: DepsMut,
    sender: Addr,
    pair_addr: Addr,
    share_index: u8,
    twap_period: u64,
    now: u64,
) -> Result<Response, ContractError> {
    OWNABLE.assert_owner(deps.storage, &sender)?;

    SHARE_ORACLE.config(
        deps.storage,
        &deps.querier,
        &pair_addr,
        share_index,
        twap_period,
        now,
    )?;

    Ok(Response::new()
        .add_attribute("action", "config_share_oracle")
        .add_attribute("pair_address", pair_addr)
        .add_attribute("base_index", share_index.to_string())
        .add_attribute("twap_period", twap_period.to_string()))
}

pub fn execute_config_synth_oracle(
    deps: DepsMut,
    sender: Addr,
    pair_addr: Addr,
    synth_index: u8,
    twap_period: u64,
    now: u64,
) -> Result<Response, ContractError> {
    OWNABLE.assert_owner(deps.storage, &sender)?;

    SYNTH_ORACLE.config(
        deps.storage,
        &deps.querier,
        &pair_addr,
        synth_index,
        twap_period,
        now,
    )?;

    EPOCH.config_oracle(deps.storage, &deps.querier, &pair_addr, synth_index, now)?;

    Ok(Response::new()
        .add_attribute("action", "config_synth_oracle")
        .add_attribute("pair_address", pair_addr)
        .add_attribute("base_index", synth_index.to_string())
        .add_attribute("twap_period", twap_period.to_string()))
}

// ====== READ FUNCTIONS ======
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetPoolInfo {} => to_binary(&query_pool_info(deps, env)?),
        QueryMsg::GetUserInfo { address } => to_binary(&query_user_info(deps, address)?),
        QueryMsg::CalcMint { collateral_amount } => {
            to_binary(&query_calc_mint(deps, collateral_amount)?)
        }
        QueryMsg::CalcRedeem { synth_amount } => to_binary(&query_calc_redeem(deps, synth_amount)?),
        QueryMsg::GetPrice {} => to_binary(&query_get_price(deps)?),
    }
}

fn query_pool_info(deps: Deps, env: Env) -> StdResult<PoolInfoResponse> {
    let pool = POOL.pool.load(deps.storage)?;

    // query balance of native token (or coin)
    let collateral_balance = deps
        .querier
        .query_balance(&env.contract.address, &pool.collateral_denom)?
        .amount;
    let synth_oracle = SYNTH_ORACLE.get_state(deps.storage)?;
    let share_oracle = SHARE_ORACLE.get_state(deps.storage)?;

    Ok(PoolInfoResponse {
        collateral_denom: pool.collateral_denom.clone(),
        collateral_balance: collateral_balance - pool.total_fee - pool.total_unclaimed_collateral,
        synth: pool.synth,
        share: pool.share,
        collateral_ratio: pool.collateral_ratio,
        last_refresh_collateral_ratio: pool.last_refresh_collateral_ratio,
        collateral_ratio_step: pool.collateral_ratio_step,
        refresh_collateral_ratio_cooldown: pool.refresh_collateral_ratio_cooldown,
        price_band: pool.price_band,
        minting_fee: pool.minting_fee,
        redemption_fee: pool.redemption_fee,
        total_unclaimed_collateral: pool.total_unclaimed_collateral,
        total_unclaimed_synth: pool.total_unclaimed_synth,
        total_unclaimed_share: pool.total_unclaimed_share,
        oracle: OracleInfoResponse {
            share: share_oracle,
            synth: synth_oracle,
        },
        min_collateral_ratio: pool.min_collateral_ratio,
        mint_paused: pool.mint_paused,
        redeem_paused: pool.redeem_paused,
        owner: OWNABLE.query_owner(deps.storage)?,
    })
}

fn query_user_info(deps: Deps, address: Addr) -> StdResult<UserInfo> {
    let state = POOL.user.load(deps.storage, &address)?;
    Ok(state)
}

fn query_calc_mint(deps: Deps, collateral_amount: Uint128) -> StdResult<CalcMintResult> {
    let pool = POOL.pool.load(deps.storage)?;
    Ok(pool.calc_mint(collateral_amount))
}

fn query_calc_redeem(deps: Deps, synth_amount: Uint128) -> StdResult<CalcRedeemResult> {
    let pool = POOL.pool.load(deps.storage)?;
    let share_price = SHARE_ORACLE.get_spot_price(deps.storage, &deps.querier)?;

    Ok(pool.calc_redeem(synth_amount, share_price))
}

fn query_get_price(deps: Deps) -> StdResult<GetPriceResult> {
    let synth_spot = SYNTH_ORACLE.get_spot_price(deps.storage, &deps.querier)?;
    let share_spot = SHARE_ORACLE.get_spot_price(deps.storage, &deps.querier)?;
    let share_twap = SHARE_ORACLE
        .get_twap(deps.storage)
        .map(|(twap, _)| twap)
        .ok();
    let synth_twap = SYNTH_ORACLE
        .get_twap(deps.storage)
        .map(|(twap, _)| twap)
        .ok();
    Ok(GetPriceResult {
        share_spot,
        synth_spot,
        share_twap,
        synth_twap,
    })
}

fn execute_update_epoch(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let pool = POOL.pool.load(deps.storage)?;
    EPOCH
        .next_epoch(deps.storage, &deps.querier, env, &pool)
        .map_err(|e| e.into())
}
