use crate::traits::chain_context::IbcChainContext;
use crate::traits::core::Async;
use crate::traits::relay_context::RelayContext;
use crate::types::aliases::ClientId;

pub struct SourceTarget;

pub struct DestinationTarget;

pub trait ChainTarget<Relay: RelayContext>: Async + private::Sealed {
    type TargetChain: IbcChainContext<Self::CounterpartyChain, Error = Relay::Error>;

    type CounterpartyChain: IbcChainContext<Self::TargetChain, Error = Relay::Error>;

    fn target_chain(context: &Relay) -> &Self::TargetChain;

    fn counterparty_chain(context: &Relay) -> &Self::CounterpartyChain;

    fn target_client_id(context: &Relay) -> &ClientId<Self::TargetChain, Self::CounterpartyChain>;

    fn counterparty_client_id(
        context: &Relay,
    ) -> &ClientId<Self::CounterpartyChain, Self::TargetChain>;
}

impl private::Sealed for SourceTarget {}
impl private::Sealed for DestinationTarget {}

impl<Relay: RelayContext> ChainTarget<Relay> for SourceTarget {
    type TargetChain = Relay::SrcChain;
    type CounterpartyChain = Relay::DstChain;

    fn target_chain(context: &Relay) -> &Self::TargetChain {
        context.source_chain()
    }

    fn counterparty_chain(context: &Relay) -> &Self::CounterpartyChain {
        context.destination_chain()
    }

    fn target_client_id(context: &Relay) -> &ClientId<Self::TargetChain, Self::CounterpartyChain> {
        context.source_client_id()
    }

    fn counterparty_client_id(
        context: &Relay,
    ) -> &ClientId<Self::CounterpartyChain, Self::TargetChain> {
        context.destination_client_id()
    }
}

impl<Relay: RelayContext> ChainTarget<Relay> for DestinationTarget {
    type TargetChain = Relay::DstChain;
    type CounterpartyChain = Relay::SrcChain;

    fn target_chain(context: &Relay) -> &Self::TargetChain {
        context.destination_chain()
    }

    fn counterparty_chain(context: &Relay) -> &Self::CounterpartyChain {
        context.source_chain()
    }

    fn target_client_id(context: &Relay) -> &ClientId<Self::TargetChain, Self::CounterpartyChain> {
        context.destination_client_id()
    }

    fn counterparty_client_id(
        context: &Relay,
    ) -> &ClientId<Self::CounterpartyChain, Self::TargetChain> {
        context.source_client_id()
    }
}

mod private {
    pub trait Sealed {}
}