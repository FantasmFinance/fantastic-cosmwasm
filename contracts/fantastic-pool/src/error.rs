use cosmwasm_std::{Addr, StdError, Timestamp};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Ownable:{0}")]
    Ownable(#[from] ownable::Error),

    #[error("ParseReply: {0}")]
    ParseReplyError(#[from] cw_utils::ParseReplyError),

    #[error("Epoch:{0}")]
    Epoch(#[from] crate::epoch::Error),

    #[error("Share address is already set")]
    ShareAlreadySet {},

    #[error("Synth address is already set")]
    SynthAlreadySet {},

    #[error("Price is unvailable or outdated")]
    PriceUnavailableOrOutdated {},

    #[error("Minting is paused")]
    MintingPaused {},

    #[error("Redemption is paused")]
    RedemptionPaused {},

    #[error("Slippage reached")]
    SlippageReached {},

    #[error("No collateral sent")]
    MintInvalidCollateralAmount {},

    #[error("Incorrect synth token, want {want}, user send {send}")]
    RedeemInvalidSynthInput { want: Addr, send: Addr },

    #[error("Invalid synth input, no CW20 sent")]
    RedeemNoCw20Sent {},

    #[error("Cannot redeem zero amount")]
    RedeemEmptyAmount {},

    #[error("Collect and mint/redeem cannot happen in the same block")]
    CollectTooEarly {},

    #[error("Collateral ratio is cooling down")]
    CollateralRatioRefreshCooldown,

    #[error("Cannot update TWAP before {time}")]
    TwapPeriodNotElapsed { time: Timestamp },
}

impl From<ContractError> for StdError {
    fn from(err: ContractError) -> Self {
        Self::generic_err(err.to_string())
    }
}
