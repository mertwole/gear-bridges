#![no_std]

use cell::RefCell;
use sails_rs::{
    gstd::{ExecContext, GStdExecContext},
    prelude::*,
};
use state::EndpointList;

pub mod error;
pub mod service;
pub mod state;

#[cfg(test)]
pub mod tests;

pub struct HistoricalProxyProgram(RefCell<state::ProxyState>);

#[sails_rs::program]
impl HistoricalProxyProgram {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let exec_context = GStdExecContext::new();
        Self(RefCell::new(state::ProxyState {
            admin: exec_context.actor_id(),
            endpoints: EndpointList::new(),
        }))
    }

    pub fn historical_proxy(&self) -> service::HistoricalProxyService<GStdExecContext> {
        service::HistoricalProxyService::new(&self.0, GStdExecContext::new())
    }
}
