use crate::Room;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RoomStore(Vec<Room>);

impl RoomStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn as_slice(&self) -> &[Room] {
        &self.0
    }
    pub fn iter(&self) -> std::slice::Iter<'_, Room> {
        self.0.iter()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn get(&self, index: usize) -> Option<&Room> {
        self.0.get(index)
    }
    pub fn position(&self, predicate: impl FnMut(&Room) -> bool) -> Option<usize> {
        self.0.iter().position(predicate)
    }
    pub(crate) fn as_mut_slice(&mut self) -> &mut [Room] {
        &mut self.0
    }
    pub(crate) fn push_owned(&mut self, room: Room) {
        self.0.push(room);
    }
    pub(crate) fn retain_owned(&mut self, predicate: impl FnMut(&Room) -> bool) {
        self.0.retain(predicate);
    }
}

impl std::ops::Deref for RoomStore {
    type Target = [Room];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}
impl std::ops::Index<usize> for RoomStore {
    type Output = Room;
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}
impl<'a> IntoIterator for &'a RoomStore {
    type Item = &'a Room;
    type IntoIter = std::slice::Iter<'a, Room>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
impl From<Vec<Room>> for RoomStore {
    fn from(value: Vec<Room>) -> Self {
        Self(value)
    }
}
impl From<RoomStore> for Vec<Room> {
    fn from(value: RoomStore) -> Self {
        value.0
    }
}
