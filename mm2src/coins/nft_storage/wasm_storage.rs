use crate::nft::nft_structs::{Chain, Nft, NftList, NftTransferHistory, NftsTransferHistoryList};
use crate::nft_storage::{CreateNftStorageError, NftListStorageError, NftListStorageOps, NftTxHistoryStorageError,
                         NftTxHistoryStorageOps};
use crate::CoinsContext;
use async_trait::async_trait;
use derive_more::Display;
use mm2_core::mm_ctx::MmArc;
pub use mm2_db::indexed_db::InitDbResult;
use mm2_db::indexed_db::{DbIdentifier, DbInstance, DbLocked, DbTransactionError, IndexedDb, IndexedDbBuilder,
                         InitDbError, SharedDb};
use mm2_err_handle::map_mm_error::MapMmError;
use mm2_err_handle::map_to_mm::MapToMmResult;
use mm2_err_handle::prelude::MmResult;

const DB_NAME: &str = "nft_cache";
const DB_VERSION: u32 = 1;

pub type NftCacheIDBResult<T> = MmResult<T, WasmNftCacheError>;
pub type NftCacheIDBLocked<'a> = DbLocked<'a, NftCacheIDB>;

impl NftListStorageError for WasmNftCacheError {}
impl NftTxHistoryStorageError for WasmNftCacheError {}

#[derive(Debug, Display)]
pub enum WasmNftCacheError {
    ErrorSerializing(String),
    ErrorDeserializing(String),
    ErrorSaving(String),
    ErrorLoading(String),
    ErrorClearing(String),
    NotSupported(String),
    InternalError(String),
}

impl From<InitDbError> for WasmNftCacheError {
    fn from(e: InitDbError) -> Self {
        match &e {
            InitDbError::NotSupported(_) => WasmNftCacheError::NotSupported(e.to_string()),
            InitDbError::EmptyTableList
            | InitDbError::DbIsOpenAlready { .. }
            | InitDbError::InvalidVersion(_)
            | InitDbError::OpeningError(_)
            | InitDbError::TypeMismatch { .. }
            | InitDbError::UnexpectedState(_)
            | InitDbError::UpgradingError { .. } => WasmNftCacheError::InternalError(e.to_string()),
        }
    }
}

impl From<DbTransactionError> for WasmNftCacheError {
    fn from(e: DbTransactionError) -> Self {
        match e {
            DbTransactionError::ErrorSerializingItem(_) => WasmNftCacheError::ErrorSerializing(e.to_string()),
            DbTransactionError::ErrorDeserializingItem(_) => WasmNftCacheError::ErrorDeserializing(e.to_string()),
            DbTransactionError::ErrorUploadingItem(_) => WasmNftCacheError::ErrorSaving(e.to_string()),
            DbTransactionError::ErrorGettingItems(_) | DbTransactionError::ErrorCountingItems(_) => {
                WasmNftCacheError::ErrorLoading(e.to_string())
            },
            DbTransactionError::ErrorDeletingItems(_) => WasmNftCacheError::ErrorClearing(e.to_string()),
            DbTransactionError::NoSuchTable { .. }
            | DbTransactionError::ErrorCreatingTransaction(_)
            | DbTransactionError::ErrorOpeningTable { .. }
            | DbTransactionError::ErrorSerializingIndex { .. }
            | DbTransactionError::UnexpectedState(_)
            | DbTransactionError::TransactionAborted
            | DbTransactionError::MultipleItemsByUniqueIndex { .. }
            | DbTransactionError::NoSuchIndex { .. }
            | DbTransactionError::InvalidIndex { .. } => WasmNftCacheError::InternalError(e.to_string()),
        }
    }
}

pub struct NftCacheIDB {
    inner: IndexedDb,
}

#[async_trait]
impl DbInstance for NftCacheIDB {
    fn db_name() -> &'static str { DB_NAME }

    async fn init(db_id: DbIdentifier) -> InitDbResult<Self> {
        // todo add tables for each chain?
        let inner = IndexedDbBuilder::new(db_id).with_version(DB_VERSION).build().await?;
        Ok(NftCacheIDB { inner })
    }
}

impl NftCacheIDB {
    fn get_inner(&self) -> &IndexedDb { &self.inner }
}

#[derive(Clone)]
pub struct IndexedDbNftStorage {
    db: SharedDb<NftCacheIDB>,
}

impl IndexedDbNftStorage {
    pub fn new(ctx: &MmArc) -> MmResult<Self, CreateNftStorageError> {
        let coins_ctx = CoinsContext::from_ctx(ctx).map_to_mm(CreateNftStorageError::Internal)?;
        Ok(IndexedDbNftStorage {
            db: coins_ctx.nft_cache_db.clone(),
        })
    }

    async fn lock_db(&self) -> NftCacheIDBResult<NftCacheIDBLocked<'_>> {
        self.db.get_or_initialize().await.mm_err(WasmNftCacheError::from)
    }
}

#[async_trait]
impl NftListStorageOps for IndexedDbNftStorage {
    type Error = WasmNftCacheError;

    async fn init(&self, _chain: Chain) -> MmResult<(), Self::Error> { todo!() }

    async fn is_initialized_for(&self, _chain: Chain) -> MmResult<bool, Self::Error> { todo!() }

    async fn get_nft_list(&self, _chain: Chain) -> MmResult<NftList, Self::Error> { todo!() }

    async fn add_nfts_to_list<I>(&self, _chain: Chain, _nfts: I) -> MmResult<(), Self::Error>
    where
        I: IntoIterator<Item = Nft> + Send + 'static,
        I::IntoIter: Send,
    {
        todo!()
    }

    async fn remove_nft_from_list(&self, _nft: Nft) -> MmResult<(), Self::Error> { todo!() }
}

#[async_trait]
impl NftTxHistoryStorageOps for IndexedDbNftStorage {
    type Error = WasmNftCacheError;

    async fn init(&self, _chain: Chain) -> MmResult<(), Self::Error> { todo!() }

    async fn is_initialized_for(&self, _chain: Chain) -> MmResult<bool, Self::Error> { todo!() }

    async fn get_tx_history(&self, _chain: Chain) -> MmResult<NftsTransferHistoryList, Self::Error> { todo!() }

    async fn add_txs_to_history<I>(&self, _chain: Chain, _nfts: I) -> MmResult<(), Self::Error>
    where
        I: IntoIterator<Item = NftTransferHistory> + Send + 'static,
        I::IntoIter: Send,
    {
        todo!()
    }
}
