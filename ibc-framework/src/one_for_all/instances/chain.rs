use crate::all_for_one::traits::base::AfoChainContext;
use crate::one_for_all::traits::chain::OfaChain;
use crate::one_for_all::types::chain::OfaChainWrapper;

pub fn ofa_to_afo_chain<Chain>(chain: Chain) -> impl AfoChainContext
where
    Chain: OfaChain,
{
    OfaChainWrapper { chain }
}
