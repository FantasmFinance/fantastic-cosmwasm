///! user stake their token and receive position token, which is CW20 itself
///! they can redeem these token to withdraw their stake. Or transfer to other
///! Even stake them to other stake contract which accept these token
use crate::error::ContractError;
use crate::msg::{Cw20ReceiveCallbackMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::stake::STAKE;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response,
    StdError, StdResult, SubMsg, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Cw20CoinVerified, Cw20ReceiveMsg, MinterResponse, TokenInfoResponse};
use cw_utils::parse_reply_instantiate_data;
use ownable::OWNABLE;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:fantastic-stake";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
const INSTANTIATE_TOKEN_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    OWNABLE.initialize(deps.storage, info.sender)?;
    STAKE.initialize(
        deps.storage,
        env.block.time.seconds(),
        msg.want_token.clone(),
        msg.reward_token,
    )?;

    let want_token_info: TokenInfoResponse = deps
        .querier
        .query_wasm_smart(msg.want_token.clone(), &cw20::Cw20QueryMsg::TokenInfo {})?;

    let init_token_msg = WasmMsg::Instantiate {
        admin: None,
        code_id: msg.token_code_id,
        msg: to_binary(&cw20_base::msg::InstantiateMsg {
            decimals: want_token_info.decimals,
            name: msg.token_name,
            symbol: msg.token_symbol,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                cap: None,
                minter: env.contract.address.to_string(),
            }),
            marketing: None,
        })?,
        funds: vec![],
        label: String::from("Fantastic stake token"),
    };

    let sub_msg = SubMsg {
        gas_limit: None,
        msg: init_token_msg.into(),
        id: INSTANTIATE_TOKEN_REPLY_ID,
        reply_on: cosmwasm_std::ReplyOn::Success,
    };

    // let sub_msg = SubMsg::reply_on_success(init_token_msg.into(), INSTANTIATE_TOKEN_REPLY_ID);

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_submessage(sub_msg))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INSTANTIATE_TOKEN_REPLY_ID => {
            let res = parse_reply_instantiate_data(msg)?;
            let contract_addr = deps.api.addr_validate(&res.contract_address)?;
            STAKE.set_position_token(deps.storage, &contract_addr)
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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(cw20_msg) => execute_receive_cw20(deps, env, info.sender, cw20_msg),
        ExecuteMsg::Harvest { to } => STAKE.harvest(deps, env, &to.unwrap_or(info.sender)),
        ExecuteMsg::SetRewardPerSecond { reward_per_second } => {
            OWNABLE.assert_owner(deps.storage, &info.sender)?;
            STAKE.set_reward_per_second(deps, env, reward_per_second)
        }
        ExecuteMsg::SetBoostToken {
            address,
            multiplier,
        } => {
            OWNABLE.assert_owner(deps.storage, &info.sender)?;
            STAKE.set_boost_token(deps, env, address, multiplier)
        }
    }
}

fn execute_receive_cw20(
    deps: DepsMut,
    env: Env,
    token_addr: Addr,
    cw20_receive_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let token_sender = deps.api.addr_validate(&cw20_receive_msg.sender)?;
    let coin = Cw20CoinVerified {
        address: token_addr,
        amount: cw20_receive_msg.amount,
    };
    match from_binary(&cw20_receive_msg.msg) {
        Ok(Cw20ReceiveCallbackMsg::Deposit { to }) => {
            let user = to.unwrap_or(token_sender);
            STAKE.deposit(deps, env, &user, coin)
        }
        Ok(Cw20ReceiveCallbackMsg::DepositBoostToken { to }) => {
            STAKE.deposit_boost_token(deps, env, &to.unwrap_or(token_sender), coin)
        }
        Ok(Cw20ReceiveCallbackMsg::Withdraw {}) => STAKE.withdraw(deps, env, &token_sender, coin),
        Ok(Cw20ReceiveCallbackMsg::WithdrawAndHarvest {}) => {
            STAKE.withdraw_and_harvest(deps, env, &token_sender, coin)
        }
        Err(e) => return Err(ContractError::Std(e)),
    }
}

// ====== READ FUNCTIONS ======
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::PoolInfo {} => to_binary(&STAKE.get_pool(deps.storage)?),
        QueryMsg::UserInfo { user } => {
            to_binary(&STAKE.query_user_info(deps.storage, &user, env.block.time.seconds())?)
        }
        QueryMsg::PendingReward { user } => to_binary(&STAKE.pending_reward(deps, env, &user)?),
    }
}
