//! NoneOneOrMany collection type for handling none, one, or many items

use serde::{Deserialize, Serialize};

/// A type that can represent none, one item, or many items
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NoneOneOrMany<T> {
    None,
    One(T),
    Many(Vec<T>),
}

impl<T> NoneOneOrMany<T> {
    /// Create a NoneOneOrMany with no items
    pub fn none() -> Self {
        NoneOneOrMany::None
    }
    
    /// Create a NoneOneOrMany with a single item
    pub fn one(item: T) -> Self {
        NoneOneOrMany::One(item)
    }
    
    /// Create a NoneOneOrMany with multiple items
    pub fn many(items: Vec<T>) -> Self {
        NoneOneOrMany::Many(items)
    }
    
    /// Convert to a vector
    pub fn into_vec(self) -> Vec<T> {
        match self {
            NoneOneOrMany::None => vec![],
            NoneOneOrMany::One(item) => vec![item],
            NoneOneOrMany::Many(items) => items,
        }
    }
    
    /// Get as a slice
    pub fn as_slice(&self) -> &[T] {
        match self {
            NoneOneOrMany::None => &[],
            NoneOneOrMany::One(item) => std::slice::from_ref(item),
            NoneOneOrMany::Many(items) => items.as_slice(),
        }
    }
    
    /// Get the number of items
    pub fn len(&self) -> usize {
        match self {
            NoneOneOrMany::None => 0,
            NoneOneOrMany::One(_) => 1,
            NoneOneOrMany::Many(items) => items.len(),
        }
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        match self {
            NoneOneOrMany::None => true,
            NoneOneOrMany::One(_) => false,
            NoneOneOrMany::Many(items) => items.is_empty(),
        }
    }
    
    /// Check if none
    pub fn is_none(&self) -> bool {
        matches!(self, NoneOneOrMany::None)
    }
    
    /// Check if one
    pub fn is_one(&self) -> bool {
        matches!(self, NoneOneOrMany::One(_))
    }
    
    /// Check if many
    pub fn is_many(&self) -> bool {
        matches!(self, NoneOneOrMany::Many(_))
    }
    
    /// Iterate over items
    pub fn iter(&self) -> NoneOneOrManyIter<T> {
        NoneOneOrManyIter::new(self)
    }
    
    /// Map over items
    pub fn map<U, F>(self, mut f: F) -> NoneOneOrMany<U>
    where
        F: FnMut(T) -> U,
    {
        match self {
            NoneOneOrMany::None => NoneOneOrMany::None,
            NoneOneOrMany::One(item) => NoneOneOrMany::One(f(item)),
            NoneOneOrMany::Many(items) => NoneOneOrMany::Many(items.into_iter().map(f).collect()),
        }
    }
}

impl<T> Default for NoneOneOrMany<T> {
    fn default() -> Self {
        NoneOneOrMany::None
    }
}

impl<T> From<Option<T>> for NoneOneOrMany<T> {
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(item) => NoneOneOrMany::One(item),
            None => NoneOneOrMany::None,
        }
    }
}

impl<T> From<Vec<T>> for NoneOneOrMany<T> {
    fn from(items: Vec<T>) -> Self {
        match items.len() {
            0 => NoneOneOrMany::None,
            1 => NoneOneOrMany::One(items.into_iter().next().unwrap()),
            _ => NoneOneOrMany::Many(items),
        }
    }
}

impl<T> IntoIterator for NoneOneOrMany<T> {
    type Item = T;
    type IntoIter = NoneOneOrManyIntoIter<T>;
    
    fn into_iter(self) -> Self::IntoIter {
        NoneOneOrManyIntoIter::new(self)
    }
}

/// Iterator for NoneOneOrMany
pub struct NoneOneOrManyIter<'a, T> {
    inner: std::slice::Iter<'a, T>,
}

impl<'a, T> NoneOneOrManyIter<'a, T> {
    fn new(none_one_or_many: &'a NoneOneOrMany<T>) -> Self {
        Self {
            inner: none_one_or_many.as_slice().iter(),
        }
    }
}

impl<'a, T> Iterator for NoneOneOrManyIter<'a, T> {
    type Item = &'a T;
    
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

/// Owned iterator for NoneOneOrMany
pub enum NoneOneOrManyIntoIter<T> {
    None,
    One(std::option::IntoIter<T>),
    Many(std::vec::IntoIter<T>),
}

impl<T> NoneOneOrManyIntoIter<T> {
    fn new(none_one_or_many: NoneOneOrMany<T>) -> Self {
        match none_one_or_many {
            NoneOneOrMany::None => NoneOneOrManyIntoIter::None,
            NoneOneOrMany::One(item) => NoneOneOrManyIntoIter::One(Some(item).into_iter()),
            NoneOneOrMany::Many(items) => NoneOneOrManyIntoIter::Many(items.into_iter()),
        }
    }
}

impl<T> Iterator for NoneOneOrManyIntoIter<T> {
    type Item = T;
    
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            NoneOneOrManyIntoIter::None => None,
            NoneOneOrManyIntoIter::One(iter) => iter.next(),
            NoneOneOrManyIntoIter::Many(iter) => iter.next(),
        }
    }
}