use serde::{Deserialize, Serialize};

use crate::gameplay::primitives::{
    dev_card::{DevCardData, DevCardDataPlayingError, DevCardKind, UsableDevCardKind},
    resource::ResourceCollection,
};

pub type PlayerId = usize;

#[derive(Debug, Clone)]
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
        (0..self.players.len()).map(|id| self.get(id))
    }

    pub fn best_army(&self) -> Option<PlayerId> {
        self.best_army
    }

    pub fn count(&self) -> PlayerId {
        self.players.len() as PlayerId
    }

    pub fn dev_card_play(
        &mut self,
        player_id: PlayerId,
        card: UsableDevCardKind,
    ) -> Result<(), DevCardDataPlayingError> {
        if card == UsableDevCardKind::Knight {
            let candidate = self.players[player_id].dev_cards.used[UsableDevCardKind::Knight] + 1;
            let number_to_beat = match self.best_army {
                Some(id) => self.players[id].dev_cards.used[UsableDevCardKind::Knight],
                None => 2,
            };

            if candidate > number_to_beat {
                self.best_army = Some(player_id);
            }
        }

        self.players[player_id].dev_cards.move_to_used(card)
    }

    pub fn get(&self, player_id: PlayerId) -> PlayerDataProxy<'_> {
        PlayerDataProxy {
            player_id,
            container: self,
            resources: &self.players[player_id].resources,
            dev_cards: &self.players[player_id].dev_cards,
        }
    }

    pub fn get_mut(&mut self, player_id: PlayerId) -> PlayerDataProxyMut<'_> {
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
        match id_fst.cmp(&id_snd) {
            std::cmp::Ordering::Equal => panic!(
                "can't borrow mutably two identical objects; ids are: {:?} (should be two distinct)",
                ids
            ),
            std::cmp::Ordering::Less => {
                let (left, right) = self.players.split_at_mut(id_snd);
                (&mut left[id_fst], &mut right[0])
            }
            std::cmp::Ordering::Greater => {
                let (left, right) = self.players.split_at_mut(id_fst);
                (&mut right[0], &mut left[id_snd])
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
        &self.container.players[self.player_id].resources
    }

    pub fn dev_cards(&self) -> &'a DevCardData {
        &self.container.players[self.player_id].dev_cards
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
        &mut self.container.players[self.player_id].resources
    }

    pub fn dev_cards(&self) -> &DevCardData {
        &self.container.players[self.player_id].dev_cards
    }

    pub fn dev_cards_reset_queue(&mut self) {
        self.container.players[self.player_id]
            .dev_cards
            .reset_queue();
    }

    pub fn dev_cards_move_to_used(
        &mut self,
        card: UsableDevCardKind,
    ) -> Result<(), DevCardDataPlayingError> {
        self.container.dev_card_play(self.player_id, card)
    }

    pub fn dev_cards_add(&mut self, card: DevCardKind) {
        self.container.players[self.player_id].dev_cards.add(card);
    }
}
