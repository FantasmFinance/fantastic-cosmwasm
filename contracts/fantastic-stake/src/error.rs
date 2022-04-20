use cosmwasm_std::{Addr, StdError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Ownable: {0}")]
    Ownable(#[from] ownable::Error),

    #[error("ParseReply: {0}")]
    ParseReplyError(#[from] cw_utils::ParseReplyError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Invalid deposit token, want {want} but {sent} sent")]
    InvalidDepositToken { want: Addr, sent: Addr },

    #[error("Withdrawal amount larger than deposited")]
    WithdrawTooMuch,

    #[error("Must redeem position token to withdraw")]
    WithdrawInvalidPositionToken,

    #[error("You must deposit some token before boosting")]
    BoostWithEmptyDeposit,

    #[error("Invalid boost token or zero amount")]
    InvalidBoostToken {},
}
