use crate::Contact;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContactStore(Vec<Contact>);

impl ContactStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn as_slice(&self) -> &[Contact] {
        &self.0
    }
    pub fn iter(&self) -> std::slice::Iter<'_, Contact> {
        self.0.iter()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn get(&self, index: usize) -> Option<&Contact> {
        self.0.get(index)
    }
    pub fn position(&self, predicate: impl FnMut(&Contact) -> bool) -> Option<usize> {
        self.0.iter().position(predicate)
    }
    pub(crate) fn as_mut_slice(&mut self) -> &mut [Contact] {
        &mut self.0
    }
    pub(crate) fn push_owned(&mut self, contact: Contact) {
        self.0.push(contact);
    }
    pub(crate) fn retain_owned(&mut self, predicate: impl FnMut(&Contact) -> bool) {
        self.0.retain(predicate);
    }
}

impl std::ops::Deref for ContactStore {
    type Target = [Contact];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}
impl std::ops::Index<usize> for ContactStore {
    type Output = Contact;
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}
impl<'a> IntoIterator for &'a ContactStore {
    type Item = &'a Contact;
    type IntoIter = std::slice::Iter<'a, Contact>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
impl From<Vec<Contact>> for ContactStore {
    fn from(value: Vec<Contact>) -> Self {
        Self(value)
    }
}
impl From<ContactStore> for Vec<Contact> {
    fn from(value: ContactStore) -> Self {
        value.0
    }
}
