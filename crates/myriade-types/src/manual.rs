/// Jeton d'écriture d'une transaction ouverte.
#[derive(Debug, PartialEq, Eq)]
pub struct Transaction(pub u64);

/// Jeton de fermeture, tenu par celui qui a ouvert la transaction.
#[derive(Debug, PartialEq, Eq)]
pub struct TransactionSeal {
    pub tx: Transaction,
    pub close: u64,
}

impl TransactionSeal {
    pub fn transaction(&self) -> &Transaction {
        &self.tx
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionLifecycle {
    None,
    Open,
    Closing,
    RootClosing,
}

pub fn transaction_seal(token: u64, close: u64) -> TransactionSeal {
    TransactionSeal { tx: Transaction(token), close }
}

pub fn transaction_of(token: u64) -> Transaction {
    Transaction(token)
}

pub fn transaction_token(tx: &Transaction) -> u64 {
    tx.0
}

pub fn seal_tokens(seal: &TransactionSeal) -> (u64, u64) {
    (seal.tx.0, seal.close)
}

/// Ce qui ouvre et ferme des transactions (le jeu).
pub trait TransactionHost {
    fn transaction_open(&mut self) -> TransactionSeal;
    fn transaction_close(&mut self, seal: &TransactionSeal, commit: bool);
}

/// Valide sur `Ok`, annule sur `Err` ou panique.
pub fn run_transaction<H: TransactionHost + ?Sized, T, E>(
    host: &mut H,
    f: impl FnOnce(&mut H, &Transaction) -> Result<T, E>,
) -> Result<T, E> {
    struct Open<'a, H: TransactionHost + ?Sized> {
        host: &'a mut H,
        seal: TransactionSeal,
        commit: bool,
    }
    impl<H: TransactionHost + ?Sized> Drop for Open<'_, H> {
        fn drop(&mut self) {
            self.host.transaction_close(&self.seal, self.commit);
        }
    }
    let seal = host.transaction_open();
    let mut open = Open { host, seal, commit: false };
    let result = f(&mut *open.host, open.seal.transaction());
    open.commit = result.is_ok();
    result
}

/// Garde l'ordre de première apparition.
pub fn dedup_keep_order(items: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert(i.clone()));
}
