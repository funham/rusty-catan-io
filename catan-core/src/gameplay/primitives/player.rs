use serde::{Deserialize, Serialize};

use crate::gameplay::primitives::{
    dev_card::{DevCardData, DevCardDataPlayingError, DevCardKind, UsableDevCard},
    resource::ResourceCollection,
};

#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct PlayerId(u8);

impl PlayerId {
    pub const fn new(raw: u8) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u8 {
        self.0
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl From<u8> for PlayerId {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl TryFrom<usize> for PlayerId {
    type Error = std::num::TryFromIntError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Ok(Self(u8::try_from(value)?))
    }
}

impl TryFrom<i32> for PlayerId {
    type Error = std::num::TryFromIntError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Ok(Self(u8::try_from(value)?))
    }
}

impl From<PlayerId> for usize {
    fn from(value: PlayerId) -> Self {
        value.index()
    }
}

impl PartialEq<usize> for PlayerId {
    fn eq(&self, other: &usize) -> bool {
        self.index() == *other
    }
}

impl PartialEq<PlayerId> for usize {
    fn eq(&self, other: &PlayerId) -> bool {
        *self == other.index()
    }
}

impl PartialOrd<usize> for PlayerId {
    fn partial_cmp(&self, other: &usize) -> Option<std::cmp::Ordering> {
        self.index().partial_cmp(other)
    }
}

impl std::fmt::Display for PlayerId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::str::FromStr for PlayerId {
    type Err = std::num::ParseIntError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse::<u8>().map(Self)
    }
}

pub fn player_ids(count: usize) -> impl Iterator<Item = PlayerId> + Clone {
    (0..count).map(|id| PlayerId::try_from(id).expect("player count should fit in u8"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerDataContainer {
    players: Vec<PlayerData>,
    best_army: Option<PlayerId>,
}

impl PlayerDataContainer {
    pub fn new(n_players: usize) -> Self {
        Self {
            players: vec![PlayerData::default(); n_players],
            best_army: None,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = PlayerDataProxy<'_>> {
        player_ids(self.players.len()).map(|id| self.get(id))
    }

    pub fn best_army(&self) -> Option<PlayerId> {
        self.best_army
    }

    pub fn count(&self) -> usize {
        self.players.len()
    }

    pub fn dev_card_play(
        &mut self,
        player_id: impl Into<PlayerId>,
        card: UsableDevCard,
    ) -> Result<(), DevCardDataPlayingError> {
        let player_id = player_id.into();
        if card == UsableDevCard::Knight {
            let candidate =
                self.players[player_id.index()].dev_cards.used[UsableDevCard::Knight] + 1;
            let number_to_beat = match self.best_army {
                Some(id) => self.players[id.index()].dev_cards.used[UsableDevCard::Knight],
                None => 2,
            };

            if candidate > number_to_beat {
                self.best_army = Some(player_id);
            }
        }

        self.players[player_id.index()].dev_cards.move_to_used(card)
    }

    pub fn get(&self, player_id: impl Into<PlayerId>) -> PlayerDataProxy<'_> {
        let player_id = player_id.into();
        PlayerDataProxy {
            player_id,
            container: self,
            resources: &self.players[player_id.index()].resources,
            dev_cards: &self.players[player_id.index()].dev_cards,
        }
    }

    pub fn get_mut(&mut self, player_id: impl Into<PlayerId>) -> PlayerDataProxyMut<'_> {
        let player_id = player_id.into();
        PlayerDataProxyMut {
            player_id,
            container: self,
        }
    }

    pub fn get_mut_both_raw(
        &mut self,
        ids: (PlayerId, PlayerId),
    ) -> (&mut PlayerData, &mut PlayerData) {
        let (id_fst, id_snd) = ids;
        match id_fst.index().cmp(&id_snd.index()) {
            std::cmp::Ordering::Equal => panic!(
                "can't borrow mutably two identical objects; ids are: {:?} (should be two distinct)",
                ids
            ),
            std::cmp::Ordering::Less => {
                let (left, right) = self.players.split_at_mut(id_snd.index());
                (&mut left[id_fst.index()], &mut right[0])
            }
            std::cmp::Ordering::Greater => {
                let (left, right) = self.players.split_at_mut(id_fst.index());
                (&mut right[0], &mut left[id_snd.index()])
            }
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PlayerData {
    pub resources: ResourceCollection,
    pub dev_cards: DevCardData,
}

impl PlayerData {
    pub fn can_pay(&self, resources: &ResourceCollection) -> bool {
        self.resources.has_enough(resources)
    }

    pub fn receive(&mut self, resources: ResourceCollection) {
        self.resources += &resources;
    }

    pub fn pay<E>(&mut self, resources: ResourceCollection, err: E) -> Result<(), E> {
        self.resources
            .subtract_in_place(&resources)
            .map_err(|_| err)
    }
}

pub struct PlayerDataProxy<'a> {
    player_id: PlayerId,
    container: &'a PlayerDataContainer,
    pub resources: &'a ResourceCollection,
    pub dev_cards: &'a DevCardData,
}

impl<'a> PlayerDataProxy<'a> {
    pub fn resources(&self) -> &'a ResourceCollection {
        &self.container.players[self.player_id.index()].resources
    }

    pub fn dev_cards(&self) -> &'a DevCardData {
        &self.container.players[self.player_id.index()].dev_cards
    }

    pub fn has_largest_army(&self) -> bool {
        match self.container.best_army {
            Some(id) if id == self.player_id => true,
            _ => false,
        }
    }
}

pub struct PlayerDataProxyMut<'a> {
    player_id: PlayerId,
    container: &'a mut PlayerDataContainer,
}

impl<'a> PlayerDataProxyMut<'a> {
    pub fn resources(&mut self) -> &mut ResourceCollection {
        &mut self.container.players[self.player_id.index()].resources
    }

    pub fn dev_cards(&self) -> &DevCardData {
        &self.container.players[self.player_id.index()].dev_cards
    }

    pub fn dev_cards_reset_queue(&mut self) {
        self.container.players[self.player_id.index()]
            .dev_cards
            .reset_queue();
    }

    pub fn dev_cards_move_to_used(
        &mut self,
        card: UsableDevCard,
    ) -> Result<(), DevCardDataPlayingError> {
        self.container.dev_card_play(self.player_id, card)
    }

    pub fn dev_cards_add(&mut self, card: DevCardKind) {
        self.container.players[self.player_id.index()]
            .dev_cards
            .add(card);
    }
}

#[cfg(test)]
mod tests {
    use super::{PlayerId, player_ids};

    #[test]
    fn player_id_is_transparent_u8_for_snapshots() {
        let raw = serde_json::to_string(&PlayerId::new(3)).unwrap();
        assert_eq!(raw, "3");

        let restored: PlayerId = serde_json::from_str(&raw).unwrap();
        assert_eq!(restored.get(), 3);
        assert_eq!(restored.index(), 3);
    }

    #[test]
    fn player_ids_iterates_dense_zero_based_ids() {
        let ids = player_ids(4).map(PlayerId::get).collect::<Vec<_>>();
        assert_eq!(ids, vec![0, 1, 2, 3]);
    }
}
