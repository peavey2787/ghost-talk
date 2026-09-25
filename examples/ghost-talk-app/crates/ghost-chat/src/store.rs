use crate::Chat;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChatStore(Vec<Chat>);

impl ChatStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn as_slice(&self) -> &[Chat] {
        &self.0
    }
    pub fn iter(&self) -> std::slice::Iter<'_, Chat> {
        self.0.iter()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn get(&self, index: usize) -> Option<&Chat> {
        self.0.get(index)
    }
    pub fn position(&self, predicate: impl FnMut(&Chat) -> bool) -> Option<usize> {
        self.0.iter().position(predicate)
    }
    pub(crate) fn as_mut_slice(&mut self) -> &mut [Chat] {
        &mut self.0
    }
    pub(crate) fn push_owned(&mut self, chat: Chat) {
        self.0.push(chat);
    }
    pub(crate) fn insert_owned(&mut self, index: usize, chat: Chat) {
        self.0.insert(index, chat);
    }
    pub(crate) fn retain_owned(&mut self, predicate: impl FnMut(&Chat) -> bool) {
        self.0.retain(predicate);
    }
}

impl std::ops::Deref for ChatStore {
    type Target = [Chat];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl std::ops::Index<usize> for ChatStore {
    type Output = Chat;
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl<'a> IntoIterator for &'a ChatStore {
    type Item = &'a Chat;
    type IntoIter = std::slice::Iter<'a, Chat>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl From<Vec<Chat>> for ChatStore {
    fn from(value: Vec<Chat>) -> Self {
        Self(value)
    }
}
impl From<ChatStore> for Vec<Chat> {
    fn from(value: ChatStore) -> Self {
        value.0
    }
}
