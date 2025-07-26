//! OneOrMany collection type for handling single items or collections

use serde::{Deserialize, Serialize};

/// A type that can represent either one item or many items
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    /// Create a OneOrMany with a single item
    pub fn one(item: T) -> Self {
        OneOrMany::One(item)
    }
    
    /// Create a OneOrMany with multiple items
    pub fn many(items: Vec<T>) -> Self {
        OneOrMany::Many(items)
    }
    
    /// Convert to a vector
    pub fn into_vec(self) -> Vec<T> {
        match self {
            OneOrMany::One(item) => vec![item],
            OneOrMany::Many(items) => items,
        }
    }
    
    /// Get as a slice
    pub fn as_slice(&self) -> &[T] {
        match self {
            OneOrMany::One(item) => std::slice::from_ref(item),
            OneOrMany::Many(items) => items.as_slice(),
        }
    }
    
    /// Get the number of items
    pub fn len(&self) -> usize {
        match self {
            OneOrMany::One(_) => 1,
            OneOrMany::Many(items) => items.len(),
        }
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        match self {
            OneOrMany::One(_) => false,
            OneOrMany::Many(items) => items.is_empty(),
        }
    }
    
    /// Iterate over items
    pub fn iter(&self) -> OneOrManyIter<T> {
        OneOrManyIter::new(self)
    }
    
    /// Map over items
    pub fn map<U, F>(self, mut f: F) -> OneOrMany<U>
    where
        F: FnMut(T) -> U,
    {
        match self {
            OneOrMany::One(item) => OneOrMany::One(f(item)),
            OneOrMany::Many(items) => OneOrMany::Many(items.into_iter().map(f).collect()),
        }
    }
}

impl<T> From<T> for OneOrMany<T> {
    fn from(item: T) -> Self {
        OneOrMany::One(item)
    }
}

impl<T> From<Vec<T>> for OneOrMany<T> {
    fn from(items: Vec<T>) -> Self {
        OneOrMany::Many(items)
    }
}

impl<T> IntoIterator for OneOrMany<T> {
    type Item = T;
    type IntoIter = OneOrManyIntoIter<T>;
    
    fn into_iter(self) -> Self::IntoIter {
        OneOrManyIntoIter::new(self)
    }
}

/// Iterator for OneOrMany
pub struct OneOrManyIter<'a, T> {
    inner: std::slice::Iter<'a, T>,
}

impl<'a, T> OneOrManyIter<'a, T> {
    fn new(one_or_many: &'a OneOrMany<T>) -> Self {
        Self {
            inner: one_or_many.as_slice().iter(),
        }
    }
}

impl<'a, T> Iterator for OneOrManyIter<'a, T> {
    type Item = &'a T;
    
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

/// Owned iterator for OneOrMany
pub enum OneOrManyIntoIter<T> {
    One(std::option::IntoIter<T>),
    Many(std::vec::IntoIter<T>),
}

impl<T> OneOrManyIntoIter<T> {
    fn new(one_or_many: OneOrMany<T>) -> Self {
        match one_or_many {
            OneOrMany::One(item) => OneOrManyIntoIter::One(Some(item).into_iter()),
            OneOrMany::Many(items) => OneOrManyIntoIter::Many(items.into_iter()),
        }
    }
}

impl<T> Iterator for OneOrManyIntoIter<T> {
    type Item = T;
    
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            OneOrManyIntoIter::One(iter) => iter.next(),
            OneOrManyIntoIter::Many(iter) => iter.next(),
        }
    }
}