use core::fmt::Debug;

use ibc_relayer_components::core::traits::sync::Async;
use ibc_relayer_components::logger::traits::level::HasBaseLogLevels;

use crate::one_for_all::traits::relay::{OfaHomogeneousRelay, OfaRelay};
use crate::one_for_all::traits::runtime::OfaRuntime;
use crate::one_for_all::types::relay::OfaRelayWrapper;
use crate::one_for_all::types::runtime::OfaRuntimeWrapper;

pub trait OfaBiRelay: Async {
    type Error: Debug + Async;

    type Runtime: OfaRuntime;

    type Logger: HasBaseLogLevels;

    type RelayAToB: OfaRelay;

    type RelayBToA: OfaRelay<
        SrcChain = <Self::RelayAToB as OfaRelay>::DstChain,
        DstChain = <Self::RelayAToB as OfaRelay>::SrcChain,
        Error = <Self::RelayAToB as OfaRelay>::Error,
    >;

    fn runtime(&self) -> &OfaRuntimeWrapper<Self::Runtime>;

    fn runtime_error(e: <Self::Runtime as OfaRuntime>::Error) -> Self::Error;

    fn logger(&self) -> &Self::Logger;

    fn relay_a_to_b(&self) -> &OfaRelayWrapper<Self::RelayAToB>;

    fn relay_b_to_a(&self) -> &OfaRelayWrapper<Self::RelayBToA>;

    fn relay_error(e: <Self::RelayAToB as OfaRelay>::Error) -> Self::Error;
}

pub trait OfaHomogeneousBiRelay:
    OfaBiRelay<RelayAToB = Self::Relay, RelayBToA = Self::Relay>
{
    type Relay: OfaHomogeneousRelay;
}

impl<BiRelay, Relay> OfaHomogeneousBiRelay for BiRelay
where
    BiRelay: OfaBiRelay<RelayAToB = Relay, RelayBToA = Relay>,
    Relay: OfaHomogeneousRelay,
{
    type Relay = Relay;
}