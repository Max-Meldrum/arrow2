use std::{iter::FromIterator, sync::Arc};

use crate::{
    array::{Array, Builder, ToArray},
    buffer::{types::NativeType, MutableBitmap, MutableBuffer},
    datatypes::DataType,
};

use super::PrimitiveArray;

impl<T: NativeType> Primitive<T> {
    pub fn from_slice<P: AsRef<[T]>>(slice: P) -> Self {
        unsafe { Self::from_trusted_len_iter(slice.as_ref().iter().map(Some)) }
    }
}

impl<T: NativeType, P: AsRef<[Option<T>]>> From<P> for Primitive<T> {
    fn from(slice: P) -> Self {
        unsafe { Self::from_trusted_len_iter(slice.as_ref().iter().map(|x| x.as_ref())) }
    }
}

impl<T: NativeType> Primitive<T> {
    /// Creates a [`PrimitiveArray`] from an iterator of trusted length.
    /// # Safety
    /// The iterator must be [`TrustedLen`](https://doc.rust-lang.org/std/iter/trait.TrustedLen.html).
    /// I.e. that `size_hint().1` correctly reports its length.
    #[inline]
    pub unsafe fn from_trusted_len_iter<I, P>(iter: I) -> Self
    where
        P: std::borrow::Borrow<T>,
        I: IntoIterator<Item = Option<P>>,
    {
        let iterator = iter.into_iter();

        let (validity, values) = trusted_len_unzip(iterator);

        Self { values, validity }
    }

    /// Creates a [`PrimitiveArray`] from an falible iterator of trusted length.
    /// # Safety
    /// The iterator must be [`TrustedLen`](https://doc.rust-lang.org/std/iter/trait.TrustedLen.html).
    /// I.e. that `size_hint().1` correctly reports its length.
    #[inline]
    pub unsafe fn try_from_trusted_len_iter<E, I, P>(iter: I) -> Result<Self, E>
    where
        P: std::borrow::Borrow<T>,
        I: IntoIterator<Item = Result<Option<P>, E>>,
    {
        let iterator = iter.into_iter();

        let (validity, values) = try_trusted_len_unzip(iterator)?;

        Ok(Self { values, validity })
    }
}

/// Creates a Bitmap and a [`Buffer`] from an iterator of `Option`.
/// The first buffer corresponds to a bitmap buffer, the second one
/// corresponds to a values buffer.
/// # Safety
/// The caller must ensure that `iterator` is `TrustedLen`.
#[inline]
pub(crate) unsafe fn trusted_len_unzip<I, P, T>(iterator: I) -> (MutableBitmap, MutableBuffer<T>)
where
    T: NativeType,
    P: std::borrow::Borrow<T>,
    I: Iterator<Item = Option<P>>,
{
    let (_, upper) = iterator.size_hint();
    let len = upper.expect("trusted_len_unzip requires an upper limit");

    let mut null = MutableBitmap::with_capacity(len);
    let mut buffer = MutableBuffer::<T>::with_capacity(len);

    let mut dst = buffer.as_mut_ptr();
    for item in iterator {
        let item = if let Some(item) = item {
            null.push_unchecked(true);
            *item.borrow()
        } else {
            null.push_unchecked(false);
            T::default()
        };
        std::ptr::write(dst, item);
        dst = dst.add(1);
    }
    assert_eq!(
        dst.offset_from(buffer.as_ptr()) as usize,
        len,
        "Trusted iterator length was not accurately reported"
    );
    buffer.set_len(len);
    null.set_len(len);

    (null, buffer)
}

/// # Safety
/// The caller must ensure that `iterator` is `TrustedLen`.
#[inline]
pub(crate) unsafe fn try_trusted_len_unzip<E, I, P, T>(
    iterator: I,
) -> Result<(MutableBitmap, MutableBuffer<T>), E>
where
    T: NativeType,
    P: std::borrow::Borrow<T>,
    I: Iterator<Item = Result<Option<P>, E>>,
{
    let (_, upper) = iterator.size_hint();
    let len = upper.expect("trusted_len_unzip requires an upper limit");

    let mut null = MutableBitmap::with_capacity(len);
    let mut buffer = MutableBuffer::<T>::with_capacity(len);

    let mut dst = buffer.as_mut_ptr();
    for item in iterator {
        let item = if let Some(item) = item? {
            null.push_unchecked(true);
            *item.borrow()
        } else {
            null.push_unchecked(false);
            T::default()
        };
        std::ptr::write(dst, item);
        dst = dst.add(1);
    }
    assert_eq!(
        dst.offset_from(buffer.as_ptr()) as usize,
        len,
        "Trusted iterator length was not accurately reported"
    );
    buffer.set_len(len);
    null.set_len(len);

    Ok((null, buffer))
}

/// auxiliary struct used to create a [`PrimitiveArray`] out of an iterator
#[derive(Debug)]
pub struct Primitive<T: NativeType> {
    values: MutableBuffer<T>,
    validity: MutableBitmap,
}

impl<T: NativeType> Builder<T> for Primitive<T> {
    #[inline]
    fn with_capacity(capacity: usize) -> Self {
        Self {
            values: MutableBuffer::<T>::with_capacity(capacity),
            validity: MutableBitmap::with_capacity(capacity),
        }
    }

    #[inline]
    fn push(&mut self, value: Option<&T>) {
        match value {
            Some(v) => {
                self.values.push(*v);
                self.validity.push(true);
            }
            None => {
                self.values.push(T::default());
                self.validity.push(false);
            }
        }
    }
}

impl<T: NativeType> Primitive<T> {
    pub fn to(self, data_type: DataType) -> PrimitiveArray<T> {
        let validity = if self.validity.null_count() > 0 {
            Some(self.validity.into())
        } else {
            None
        };

        PrimitiveArray::<T>::from_data(data_type, self.values.into(), validity)
    }
}

impl<T: NativeType, Ptr: std::borrow::Borrow<Option<T>>> FromIterator<Ptr> for Primitive<T> {
    fn from_iter<I: IntoIterator<Item = Ptr>>(iter: I) -> Self {
        let iter = iter.into_iter();
        let (lower, _) = iter.size_hint();

        let mut validity = MutableBitmap::with_capacity(lower);

        let values: MutableBuffer<T> = iter
            .map(|item| {
                if let Some(a) = item.borrow() {
                    validity.push(true);
                    *a
                } else {
                    validity.push(false);
                    T::default()
                }
            })
            .collect();

        Self {
            values: values,
            validity,
        }
    }
}

impl<T: NativeType> ToArray for Primitive<T> {
    fn to_arc(self, data_type: &DataType) -> Arc<dyn Array> {
        Arc::new(self.to(data_type.clone()))
    }
}