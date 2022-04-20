use cosmwasm_std::{Addr, Event, MessageInfo, Response, StdError, StdResult, Storage};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub owner: Addr,
    pub pending_owner: Option<Addr>,
}

pub struct Ownable<'a>(Item<'a, State>);

const OWNABLE_NAMESPACE: &str = "_ownable";

impl<'a> Ownable<'a> {
    pub const fn new() -> Self {
        Ownable(Item::new(OWNABLE_NAMESPACE))
    }

    pub fn initialize(&self, storage: &'_ mut dyn Storage, owner: Addr) -> StdResult<()> {
        self.0.save(
            storage,
            &State {
                owner,
                pending_owner: None,
            },
        )
    }

    pub fn is_owner(&self, storage: &dyn Storage, caller: &Addr) -> StdResult<bool> {
        let state = self.0.load(storage)?;
        Ok(caller == &state.owner)
    }

    pub fn assert_owner(&self, storage: &dyn Storage, caller: &Addr) -> Result<(), Error> {
        if !self.is_owner(storage, caller)? {
            Err(Error::Unauthorized {})
        } else {
            Ok(())
        }
    }

    // contract endpoint
    pub fn execute_transfer_ownership(
        &self,
        storage: &mut dyn Storage,
        info: MessageInfo,
        to: Addr,
    ) -> Result<Response, Error> {
        let mut state = self.0.load(storage)?;

        if info.sender != state.owner {
            return Err(Error::Unauthorized);
        }

        state.pending_owner = Some(to.clone());

        self.0.save(storage, &state)?;

        Ok(Response::default().add_event(
            Event::new("transfer_ownership")
                .add_attribute("from", state.owner)
                .add_attribute("to", to),
        ))
    }

    pub fn execute_accept_ownership(
        &self,
        storage: &mut dyn Storage,
        info: MessageInfo,
    ) -> Result<Response, Error> {
        let mut state = self.0.load(storage)?;

        if Some(&info.sender) != state.pending_owner.as_ref() {
            return Err(Error::Unauthorized);
        }

        let old_owner = std::mem::replace(&mut state.owner, info.sender);
        state.pending_owner = None;
        self.0.save(storage, &state)?;

        Ok(Response::default().add_event(
            Event::new("accept_ownership")
                .add_attribute("from", old_owner)
                .add_attribute("to", state.owner),
        ))
    }

    pub fn query_owner(&self, storage: &dyn Storage) -> StdResult<Addr> {
        let state = self.0.load(storage)?;
        Ok(state.owner)
    }
}

pub const OWNABLE: Ownable = Ownable::new();
