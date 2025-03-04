use crate::{utility::reflect_hasher, Reflect, ReflectMut, ReflectOwned, ReflectRef, TypeInfo};
use std::{
    any::{Any, TypeId},
    fmt::Debug,
    hash::{Hash, Hasher},
};

/// A trait used to power [array-like] operations via [reflection].
///
/// This corresponds to true Rust arrays like `[T; N]`,
/// but also to any fixed-size linear sequence types.
/// It is expected that implementors of this trait uphold this contract
/// and maintain a fixed size as returned by the [`Array::len`] method.
///
/// Due to the [type-erasing] nature of the reflection API as a whole,
/// this trait does not make any guarantees that the implementor's elements
/// are homogenous (i.e. all the same type).
///
/// This trait has a blanket implementation over Rust arrays of up to 32 items.
/// This implementation can technically contain more than 32,
/// but the blanket [`GetTypeRegistration`] is only implemented up to the 32
/// item limit due to a [limitation] on [`Deserialize`].
///
/// # Example
///
/// ```
/// use bevy_reflect::{Reflect, Array};
///
/// let foo: &dyn Array = &[123_u32, 456_u32, 789_u32];
/// assert_eq!(foo.len(), 3);
///
/// let field: &dyn Reflect = foo.get(0).unwrap();
/// assert_eq!(field.downcast_ref::<u32>(), Some(&123));
/// ```
///
/// [array-like]: https://doc.rust-lang.org/book/ch03-02-data-types.html#the-array-type
/// [reflection]: crate
/// [`List`]: crate::List
/// [type-erasing]: https://doc.rust-lang.org/book/ch17-02-trait-objects.html
/// [`GetTypeRegistration`]: crate::GetTypeRegistration
/// [limitation]: https://github.com/serde-rs/serde/issues/1937
/// [`Deserialize`]: ::serde::Deserialize
pub trait Array: Reflect {
    /// Returns a reference to the element at `index`, or `None` if out of bounds.
    fn get(&self, index: usize) -> Option<&dyn Reflect>;

    /// Returns a mutable reference to the element at `index`, or `None` if out of bounds.
    fn get_mut(&mut self, index: usize) -> Option<&mut dyn Reflect>;

    /// Returns the number of elements in the array.
    fn len(&self) -> usize;

    /// Returns `true` if the collection contains no elements.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns an iterator over the array.
    fn iter(&self) -> ArrayIter;

    /// Drain the elements of this array to get a vector of owned values.
    fn drain(self: Box<Self>) -> Vec<Box<dyn Reflect>>;

    /// Clones the list, producing a [`DynamicArray`].
    fn clone_dynamic(&self) -> DynamicArray {
        DynamicArray {
            represented_type: self.get_represented_type_info(),
            values: self.iter().map(|value| value.clone_value()).collect(),
        }
    }
}

/// A container for compile-time array info.
#[derive(Clone, Debug)]
pub struct ArrayInfo {
    type_name: &'static str,
    type_id: TypeId,
    item_type_name: &'static str,
    item_type_id: TypeId,
    capacity: usize,
    #[cfg(feature = "documentation")]
    docs: Option<&'static str>,
}

impl ArrayInfo {
    /// Create a new [`ArrayInfo`].
    ///
    /// # Arguments
    ///
    /// * `capacity`: The maximum capacity of the underlying array.
    ///
    pub fn new<TArray: Array, TItem: Reflect>(capacity: usize) -> Self {
        Self {
            type_name: std::any::type_name::<TArray>(),
            type_id: TypeId::of::<TArray>(),
            item_type_name: std::any::type_name::<TItem>(),
            item_type_id: TypeId::of::<TItem>(),
            capacity,
            #[cfg(feature = "documentation")]
            docs: None,
        }
    }

    /// Sets the docstring for this array.
    #[cfg(feature = "documentation")]
    pub fn with_docs(self, docs: Option<&'static str>) -> Self {
        Self { docs, ..self }
    }

    /// The compile-time capacity of the array.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// The [type name] of the array.
    ///
    /// [type name]: std::any::type_name
    pub fn type_name(&self) -> &'static str {
        self.type_name
    }

    /// The [`TypeId`] of the array.
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Check if the given type matches the array type.
    pub fn is<T: Any>(&self) -> bool {
        TypeId::of::<T>() == self.type_id
    }

    /// The [type name] of the array item.
    ///
    /// [type name]: std::any::type_name
    pub fn item_type_name(&self) -> &'static str {
        self.item_type_name
    }

    /// The [`TypeId`] of the array item.
    pub fn item_type_id(&self) -> TypeId {
        self.item_type_id
    }

    /// Check if the given type matches the array item type.
    pub fn item_is<T: Any>(&self) -> bool {
        TypeId::of::<T>() == self.item_type_id
    }

    /// The docstring of this array, if any.
    #[cfg(feature = "documentation")]
    pub fn docs(&self) -> Option<&'static str> {
        self.docs
    }
}

/// A fixed-size list of reflected values.
///
/// This differs from [`DynamicList`] in that the size of the [`DynamicArray`]
/// is constant, whereas a [`DynamicList`] can have items added and removed.
///
/// This isn't to say that a [`DynamicArray`] is immutable— its items
/// can be mutated— just that the _number_ of items cannot change.
///
/// [`DynamicList`]: crate::DynamicList
#[derive(Debug)]
pub struct DynamicArray {
    pub(crate) represented_type: Option<&'static TypeInfo>,
    pub(crate) values: Box<[Box<dyn Reflect>]>,
}

impl DynamicArray {
    #[inline]
    pub fn new(values: Box<[Box<dyn Reflect>]>) -> Self {
        Self {
            represented_type: None,
            values,
        }
    }

    pub fn from_vec<T: Reflect>(values: Vec<T>) -> Self {
        Self {
            represented_type: None,
            values: values
                .into_iter()
                .map(|field| Box::new(field) as Box<dyn Reflect>)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        }
    }

    /// Sets the [type] to be represented by this `DynamicArray`.
    ///
    /// # Panics
    ///
    /// Panics if the given [type] is not a [`TypeInfo::Array`].
    ///
    /// [type]: TypeInfo
    pub fn set_represented_type(&mut self, represented_type: Option<&'static TypeInfo>) {
        if let Some(represented_type) = represented_type {
            assert!(
                matches!(represented_type, TypeInfo::Array(_)),
                "expected TypeInfo::Array but received: {:?}",
                represented_type
            );
        }

        self.represented_type = represented_type;
    }
}

impl Reflect for DynamicArray {
    #[inline]
    fn type_name(&self) -> &str {
        self.represented_type
            .map(|info| info.type_name())
            .unwrap_or_else(|| std::any::type_name::<Self>())
    }

    #[inline]
    fn get_represented_type_info(&self) -> Option<&'static TypeInfo> {
        self.represented_type
    }

    #[inline]
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }

    #[inline]
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[inline]
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    #[inline]
    fn into_reflect(self: Box<Self>) -> Box<dyn Reflect> {
        self
    }

    #[inline]
    fn as_reflect(&self) -> &dyn Reflect {
        self
    }

    #[inline]
    fn as_reflect_mut(&mut self) -> &mut dyn Reflect {
        self
    }

    fn apply(&mut self, value: &dyn Reflect) {
        array_apply(self, value);
    }

    #[inline]
    fn set(&mut self, value: Box<dyn Reflect>) -> Result<(), Box<dyn Reflect>> {
        *self = value.take()?;
        Ok(())
    }

    #[inline]
    fn reflect_ref(&self) -> ReflectRef {
        ReflectRef::Array(self)
    }

    #[inline]
    fn reflect_mut(&mut self) -> ReflectMut {
        ReflectMut::Array(self)
    }

    #[inline]
    fn reflect_owned(self: Box<Self>) -> ReflectOwned {
        ReflectOwned::Array(self)
    }

    #[inline]
    fn clone_value(&self) -> Box<dyn Reflect> {
        Box::new(self.clone_dynamic())
    }

    #[inline]
    fn reflect_hash(&self) -> Option<u64> {
        array_hash(self)
    }

    fn reflect_partial_eq(&self, value: &dyn Reflect) -> Option<bool> {
        array_partial_eq(self, value)
    }

    #[inline]
    fn is_dynamic(&self) -> bool {
        true
    }
}

impl Array for DynamicArray {
    #[inline]
    fn get(&self, index: usize) -> Option<&dyn Reflect> {
        self.values.get(index).map(|value| &**value)
    }

    #[inline]
    fn get_mut(&mut self, index: usize) -> Option<&mut dyn Reflect> {
        self.values.get_mut(index).map(|value| &mut **value)
    }

    #[inline]
    fn len(&self) -> usize {
        self.values.len()
    }

    #[inline]
    fn iter(&self) -> ArrayIter {
        ArrayIter::new(self)
    }

    #[inline]
    fn drain(self: Box<Self>) -> Vec<Box<dyn Reflect>> {
        self.values.into_vec()
    }

    #[inline]
    fn clone_dynamic(&self) -> DynamicArray {
        DynamicArray {
            represented_type: self.represented_type,
            values: self
                .values
                .iter()
                .map(|value| value.clone_value())
                .collect(),
        }
    }
}

/// An iterator over an [`Array`].
pub struct ArrayIter<'a> {
    array: &'a dyn Array,
    index: usize,
}

impl<'a> ArrayIter<'a> {
    /// Creates a new [`ArrayIter`].
    #[inline]
    pub const fn new(array: &'a dyn Array) -> ArrayIter {
        ArrayIter { array, index: 0 }
    }
}

impl<'a> Iterator for ArrayIter<'a> {
    type Item = &'a dyn Reflect;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let value = self.array.get(self.index);
        self.index += 1;
        value
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = self.array.len();
        (size, Some(size))
    }
}

impl<'a> ExactSizeIterator for ArrayIter<'a> {}

/// Returns the `u64` hash of the given [array](Array).
#[inline]
pub fn array_hash<A: Array>(array: &A) -> Option<u64> {
    let mut hasher = reflect_hasher();
    std::any::Any::type_id(array).hash(&mut hasher);
    array.len().hash(&mut hasher);
    for value in array.iter() {
        hasher.write_u64(value.reflect_hash()?);
    }
    Some(hasher.finish())
}

/// Applies the reflected [array](Array) data to the given [array](Array).
///
/// # Panics
///
/// * Panics if the two arrays have differing lengths.
/// * Panics if the reflected value is not a [valid array](ReflectRef::Array).
///
#[inline]
pub fn array_apply<A: Array>(array: &mut A, reflect: &dyn Reflect) {
    if let ReflectRef::Array(reflect_array) = reflect.reflect_ref() {
        if array.len() != reflect_array.len() {
            panic!("Attempted to apply different sized `Array` types.");
        }
        for (i, value) in reflect_array.iter().enumerate() {
            let v = array.get_mut(i).unwrap();
            v.apply(value);
        }
    } else {
        panic!("Attempted to apply a non-`Array` type to an `Array` type.");
    }
}

/// Compares two [arrays](Array) (one concrete and one reflected) to see if they
/// are equal.
///
/// Returns [`None`] if the comparison couldn't even be performed.
#[inline]
pub fn array_partial_eq<A: Array>(array: &A, reflect: &dyn Reflect) -> Option<bool> {
    match reflect.reflect_ref() {
        ReflectRef::Array(reflect_array) if reflect_array.len() == array.len() => {
            for (a, b) in array.iter().zip(reflect_array.iter()) {
                let eq_result = a.reflect_partial_eq(b);
                if let failed @ (Some(false) | None) = eq_result {
                    return failed;
                }
            }
        }
        _ => return Some(false),
    }

    Some(true)
}

/// The default debug formatter for [`Array`] types.
///
/// # Example
/// ```
/// use bevy_reflect::Reflect;
///
/// let my_array: &dyn Reflect = &[1, 2, 3];
/// println!("{:#?}", my_array);
///
/// // Output:
///
/// // [
/// //   1,
/// //   2,
/// //   3,
/// // ]
/// ```
#[inline]
pub fn array_debug(dyn_array: &dyn Array, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let mut debug = f.debug_list();
    for item in dyn_array.iter() {
        debug.entry(&item as &dyn Debug);
    }
    debug.finish()
}
