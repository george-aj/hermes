use crate::tagged::mono::Tagged;
use crate::types::wallet::Wallet;

pub struct ChainWallets {
    pub validator: Wallet,
    pub relayer: Wallet,
    pub user1: Wallet,
    pub user2: Wallet,
}

impl<'a, Chain> Tagged<Chain, &'a ChainWallets> {
    pub fn validator(&self) -> Tagged<Chain, &Wallet> {
        self.map_ref(|w| &w.validator)
    }

    pub fn relayer(&self) -> Tagged<Chain, &Wallet> {
        self.map_ref(|w| &w.relayer)
    }

    pub fn user1(&self) -> Tagged<Chain, &Wallet> {
        self.map_ref(|w| &w.user1)
    }

    pub fn user2(&self) -> Tagged<Chain, &Wallet> {
        self.map_ref(|w| &w.user2)
    }
}