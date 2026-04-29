use std::{fs::File, io::BufReader};

use serde::{Deserialize, Serialize};
use serde_with::{Seq, serde_as};

use crate::{
    gameplay::{
        field::{BoardArrangement, PortMap},
        primitives::{Tile, resource::Resource},
    },
    math::dice::DiceVal,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum HexTypeJsonVal {
    Desert(String),
    Resource((Resource, u8)),
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
struct FieldArrangementJsonVal {
    field_radius: u8,
    tile_info: Vec<HexTypeJsonVal>,
    #[serde(default)]
    #[serde_as(as = "Seq<(_, _)>")]
    port_map: PortMap,
}

impl Serialize for BoardArrangement {
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
                Tile::River { number } => HexTypeJsonVal::Resource((Resource::Ore, number.into())),
            })
            .collect();

        FieldArrangementJsonVal {
            field_radius: self.field_radius,
            tile_info,
            port_map: self.ports().clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BoardArrangement {
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

        BoardArrangement::try_build(raw.field_radius, tile_info, raw.port_map)
            .map_err(|err| serde::de::Error::custom(format!("{err:?}")))
    }
}

pub fn arrangement_from_json(path: &std::path::Path) -> Option<BoardArrangement> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).ok()

    // let val: FieldArrangementJsonVal = serde_json::from_reader(reader).unwrap();
    // Some(val.into())
}

impl From<FieldArrangementJsonVal> for BoardArrangement {
    fn from(json: FieldArrangementJsonVal) -> Self {
        Self::try_build(
            json.field_radius,
            json.tile_info
                .into_iter()
                .map(|h| match h {
                    HexTypeJsonVal::Desert(_) => Tile::Desert,
                    HexTypeJsonVal::Resource((resource, tilenum)) => Tile::Resource {
                        resource,
                        number: DiceVal::try_from(tilenum).unwrap(),
                    },
                })
                .collect(),
            json.port_map,
        )
        .unwrap()
    }
}
