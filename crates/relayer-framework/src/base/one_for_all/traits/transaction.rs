use async_trait::async_trait;
use core::fmt::Debug;
use core::time::Duration;

use crate::base::core::traits::sync::Async;
use crate::base::one_for_all::traits::runtime::OfaBaseRuntime;
use crate::base::one_for_all::types::runtime::OfaRuntimeWrapper;
use crate::std_prelude::*;

pub trait OfaTxTypes: Async {
    type Error: Async + Debug;

    type Runtime: OfaBaseRuntime;

    /**
       Corresponds to
       [`HasMessageType::Message`](crate::base::chain::traits::types::message::HasMessageType::Message).
    */
    type Message: Async;

    /**
       Corresponds to
       [`HasEventType::Event`](crate::base::chain::traits::types::event::HasEventType::Event).
    */
    type Event: Async;

    type Transaction: Async;

    type Nonce: Async;

    type Fee: Async;

    type Signer: Async;

    type TxHash: Async;

    type TxResponse: Async;
}

#[async_trait]
pub trait OfaTxContext: OfaTxTypes {
    fn runtime(&self) -> &OfaRuntimeWrapper<Self::Runtime>;

    fn runtime_error(e: <Self::Runtime as OfaBaseRuntime>::Error) -> Self::Error;

    fn tx_no_response_error(tx_hash: &Self::TxHash) -> Self::Error;

    fn tx_size(tx: &Self::Transaction) -> usize;

    fn get_signer(&self) -> &Self::Signer;

    fn fee_for_simulation(&self) -> &Self::Fee;

    fn poll_timeout(&self) -> Duration;

    fn poll_backoff(&self) -> Duration;

    async fn encode_tx(
        &self,
        signer: &Self::Signer,
        nonce: &Self::Nonce,
        fee: &Self::Fee,
        messages: &[Self::Message],
    ) -> Result<Self::Transaction, Self::Error>;

    async fn submit_tx(&self, tx: &Self::Transaction) -> Result<Self::TxHash, Self::Error>;

    async fn estimate_tx_fee(&self, tx: &Self::Transaction) -> Result<Self::Fee, Self::Error>;

    async fn query_tx_response(
        &self,
        tx_hash: &Self::TxHash,
    ) -> Result<Option<Self::TxResponse>, Self::Error>;

    async fn query_nonce(&self, signer: &Self::Signer) -> Result<Self::Nonce, Self::Error>;

    fn mutex_for_nonce_allocation(
        &self,
        signer: &Self::Signer,
    ) -> &<Self::Runtime as OfaBaseRuntime>::Mutex<()>;

    fn parse_tx_response_as_events(
        response: Self::TxResponse,
    ) -> Result<Vec<Vec<Self::Event>>, Self::Error>;
}