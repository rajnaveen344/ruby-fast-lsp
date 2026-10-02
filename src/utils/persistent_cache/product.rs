//! The product contract: how an owner-defined product is keyed, encoded, and
//! decoded, and the typed lookup and reservation the cache returns for it.

use anyhow::Result;
use std::borrow::Cow;
use std::marker::PhantomData;
use std::sync::Arc;

use super::{
    PersistentDerivedProductCache, PersistentDerivedProductLookup,
    PersistentDerivedProductReservation, PersistentProductKind,
};

/// A derived product the persistent cache stores for its owner.
///
/// The cache owns the on-disk layout, envelope, locks, and bounds. The owner
/// owns the product's identity and payload codec, so the cache never depends
/// on the product's type.
pub trait PersistentProduct: Sized {
    /// The identity a producer derives before building the product.
    type Key: ?Sized;

    /// The namespace, envelope magic, size bound, and counters the product uses.
    const KIND: PersistentProductKind;

    /// The 64-character lowercase hexadecimal cache identity of `key`.
    fn key_cache_id(key: &Self::Key) -> Cow<'_, str>;

    /// The cache identity of this product; publication rejects a product whose
    /// identity differs from its reservation.
    fn product_cache_id(&self) -> Cow<'_, str>;

    /// Encode the payload stored inside the cache envelope.
    fn encode(&self) -> Result<Vec<u8>>;

    /// Decode a payload, rejecting one whose recorded identity differs from `key`.
    fn decode(key: &Self::Key, payload: &[u8]) -> Result<Self>;
}

pub enum PersistentProductLookup<P> {
    Hit(Arc<P>),
    Reservation(PersistentProductReservation<P>),
}

/// The exclusive right to produce and publish one missing product.
pub struct PersistentProductReservation<P> {
    inner: PersistentDerivedProductReservation,
    product: PhantomData<fn(&P)>,
}

impl PersistentDerivedProductCache {
    pub fn lookup_or_reserve<P: PersistentProduct>(
        &self,
        key: &P::Key,
    ) -> Result<PersistentProductLookup<P>> {
        let cache_id = P::key_cache_id(key);
        match self.lookup_derived_product(P::KIND, &cache_id, |payload| P::decode(key, &payload))? {
            PersistentDerivedProductLookup::Hit(product) => {
                Ok(PersistentProductLookup::Hit(Arc::new(product)))
            }
            PersistentDerivedProductLookup::Reservation(inner) => Ok(
                PersistentProductLookup::Reservation(PersistentProductReservation {
                    inner,
                    product: PhantomData,
                }),
            ),
        }
    }

    #[cfg(test)]
    pub(crate) fn product_path_for_tests<P: PersistentProduct>(
        &self,
        key: &P::Key,
    ) -> std::path::PathBuf {
        self.product_path(P::KIND, &P::key_cache_id(key))
    }
}

impl<P: PersistentProduct> PersistentProductReservation<P> {
    pub fn publish(self, product: &P) -> Result<()> {
        self.inner
            .publish_payload(&product.product_cache_id(), product.encode()?)
    }
}
