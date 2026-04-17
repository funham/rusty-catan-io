use std::{collections::BTreeMap, fs::File, io::BufReader};

use serde::{Deserialize, Serialize};

use crate::{
    gameplay::{
        field::FieldArrangement,
        primitives::{Tile, PortKind, resource::Resource},
    },
    math::dice::DiceVal,
    topology::Path,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum HexTypeJsonVal {
    Desert(String),
    Resource((Resource, u8)),
}

#[derive(Debug, Serialize, Deserialize)]
struct FieldArrangementJsonVal {
    field_radius: u8,
    tile_info: Vec<HexTypeJsonVal>,
    #[serde(default)]
    ports_info: BTreeMap<Path, PortKind>,
}

impl Serialize for FieldArrangement {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let tile_info = self
            .iter()
            .map(|hex| match hex {
                Tile::Desert => HexTypeJsonVal::Desert("desert".to_owned()),
                Tile::Resource { resource, number } => {
                    HexTypeJsonVal::Resource((resource, number.into()))
                }
                Tile::River { number } => {
                    HexTypeJsonVal::Resource((Resource::Ore, number.into()))
                }
            })
            .collect();

        FieldArrangementJsonVal {
            field_radius: self.field_radius,
            tile_info,
            ports_info: self.ports().clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FieldArrangement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = FieldArrangementJsonVal::deserialize(deserializer)?;
        let mut tile_info = Vec::with_capacity(raw.tile_info.len());

        for hex in raw.tile_info {
            match hex {
                HexTypeJsonVal::Desert(_) => tile_info.push(Tile::Desert),
                HexTypeJsonVal::Resource((resource, number)) => {
                    let number = DiceVal::try_from(number)
                        .map_err(|_| serde::de::Error::custom("invalid dice value"))?;
                    tile_info.push(Tile::Resource { resource, number });
                }
            }
        }

        FieldArrangement::new(raw.field_radius, tile_info, raw.ports_info)
            .map_err(|err| serde::de::Error::custom(format!("{err:?}")))
    }
}

pub fn arrangement_from_json(path: &std::path::Path) -> Option<FieldArrangement> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).ok()
}
